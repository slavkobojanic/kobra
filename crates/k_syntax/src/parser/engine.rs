//! The parser engine: the trivia-free token view, the event buffer with its
//! markers, token access, and the diagnostic and recovery helpers that the
//! grammar functions share.
//!
//! The grammar itself lives in the sibling modules (`items`, `statements`,
//! `expressions`, `types`, `patterns`, `styles`); they only talk to the
//! [`Parser`] through the methods defined here.
//!
//! # Events
//!
//! The parser does not build the tree directly. It records a flat list of
//! [`Event`]s: `Start(kind)`, `Token(kind)` and `Finish`. A [`Marker`]
//! remembers the position of a `Start` event whose kind is decided later
//! ([`Marker::complete`]) or never ([`Marker::abandon`]). A
//! [`CompletedMarker`] can be *preceded* by a new node, which is how
//! left-recursive constructs such as `a + b` or `f(x)` wrap an already
//! parsed operand (the `forward_parent` link, as in rust-analyzer). The
//! [`sink`](super::sink) turns the events into a rowan tree and re-inserts
//! the trivia.
//!
//! # Termination
//!
//! Every loop of the grammar consumes at least one token per iteration or
//! breaks. As a safety net, [`Parser::nth`] burns *fuel* on every lookahead
//! and [`Parser::bump`] refills it; if the fuel ever runs out, the parser is
//! stuck in a loop that does not advance, which is a bug. Debug builds
//! assert; release builds stop parsing (every lookahead then reports end of
//! file), and the sink appends the unconsumed tokens to the tree so the
//! result stays lossless.

use std::num::NonZeroU32;

use k_diagnostics::{Diagnostic, FileId, Fix, Span, TextRange, TextSize, codes};

use crate::kind::SyntaxKind;
use crate::lexer::Lexed;

/// The kind reported for the end of input.
///
/// `SourceFile` never occurs as a token, so it serves as an unambiguous
/// sentinel inside the parser without adding a variant to the public
/// [`SyntaxKind`] list.
pub(crate) const EOF: SyntaxKind = SyntaxKind::SourceFile;

/// The kind of a `Start` event that was never completed.
const TOMBSTONE: SyntaxKind = SyntaxKind::Whitespace;

/// Lookaheads allowed without consuming a token before the parser is
/// considered stuck.
const FUEL: u32 = 4096;

/// The maximum nesting depth of blocks, lists, prefix operators and other
/// recursive constructs. Deeper input is reported as K0134 and skipped
/// iteratively, so the parser's stack use is bounded on any input.
pub(crate) const MAX_DEPTH: u32 = 400;

/// A token is preceded by a line terminator (in a `Newline` token or inside
/// a block comment).
const NEWLINE_BEFORE: u8 = 1;
/// A token is preceded by trivia of any kind.
const TRIVIA_BEFORE: u8 = 2;

/// One parser event. See the [module documentation](self).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Event {
    /// Starts a node. `forward_parent` is the distance (in events) to the
    /// `Start` of a node that must wrap this one.
    Start {
        /// The node kind, or [`TOMBSTONE`] for an abandoned or not yet
        /// completed marker.
        kind: SyntaxKind,
        /// Forward distance to the `Start` of the wrapping node, if any.
        forward_parent: Option<NonZeroU32>,
    },
    /// Consumes the next non-trivia token, giving it `kind` in the tree
    /// (which differs from the lexed kind for remapped contextual keywords).
    Token {
        /// The kind the token gets in the tree.
        kind: SyntaxKind,
    },
    /// Finishes the innermost open node.
    Finish,
}

/// A started node whose kind is not decided yet.
///
/// A marker is finished with [`Marker::complete`] or [`Marker::abandon`].
/// Until then its `Start` event holds the [`TOMBSTONE`] kind, which the sink
/// skips, so an abandoned (or dropped) marker adds no node to the tree.
#[must_use]
pub(crate) struct Marker {
    /// Index of the marker's `Start` event in [`Parser::events`].
    pos: u32,
    /// The index (in the trivia-free view) of the node's first token.
    first_token: u32,
}

/// A completed node, which may still be wrapped by [`CompletedMarker::precede`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct CompletedMarker {
    /// Index of the node's `Start` event in [`Parser::events`].
    pos: u32,
    /// The kind the node was completed with.
    kind: SyntaxKind,
    /// The index (in the trivia-free view) of the node's first token.
    first_token: u32,
}

impl CompletedMarker {
    /// Returns the kind the node was completed with.
    pub(crate) fn kind(self) -> SyntaxKind {
        self.kind
    }

    /// Starts a new node that will become the parent of this one.
    pub(crate) fn precede(self, p: &mut Parser<'_>) -> Marker {
        let new = p.start_at(self.first_token);
        let distance = new.pos.saturating_sub(self.pos);
        if let Some(Event::Start { forward_parent, .. }) = p.events.get_mut(self.pos as usize) {
            *forward_parent = NonZeroU32::new(distance);
        }
        new
    }

    /// Returns the byte range of the node, from its first to its last token.
    /// Only meaningful directly after the node was completed, before more
    /// tokens are consumed.
    pub(crate) fn range(self, p: &Parser<'_>) -> TextRange {
        let start = p.token_start(self.first_token as usize);
        let end = p.prev_end().max(start);
        TextRange::new(TextSize::from(start), TextSize::from(end))
    }
}

/// The parser state shared by all grammar functions.
pub(crate) struct Parser<'t> {
    /// The source text.
    text: &'t str,
    /// The file that diagnostic spans point into.
    file: FileId,
    /// Kinds of the non-trivia tokens, followed by one [`EOF`] entry.
    kinds: Vec<SyntaxKind>,
    /// Byte ranges of the non-trivia tokens (the `EOF` entry is empty, at
    /// the end of the text).
    starts: Vec<u32>,
    /// End offsets, parallel to `starts`.
    ends: Vec<u32>,
    /// [`NEWLINE_BEFORE`] and [`TRIVIA_BEFORE`] bits of every token,
    /// parallel to `kinds`.
    flags: Vec<u8>,
    /// Index of the current token in the trivia-free view.
    pos: usize,
    /// The recorded events, in order.
    events: Vec<Event>,
    /// Parser diagnostics in the order they were reported; the caller sorts
    /// them together with the lexer's.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Lookaheads left before the parser counts as stuck; refilled to
    /// [`FUEL`] by every consumed token. A `Cell` so that lookahead can take
    /// `&self`.
    fuel: std::cell::Cell<u32>,
    /// Newlines are significant (outside parentheses, brackets and generic
    /// argument lists; rule N1).
    pub(crate) newlines: bool,
    /// Trailing lambdas are not allowed at the top level of the current
    /// expression (`expr_nt` positions such as `if` conditions).
    pub(crate) no_trailing_lambda: bool,
    /// Current nesting depth of recursive constructs.
    depth: u32,
    /// `(line start, offset)` of the last [`Parser::indentation_at`] query.
    line_cache: std::cell::Cell<(usize, usize)>,
    /// After K0134 skipped to the end of the file, the offset of the K0134:
    /// openers before it are left unclosed by the skip itself, so K0101 is
    /// not reported for them.
    skipped_to_eof_from: Option<TextSize>,
    /// The index of the first token the parser may not see: lookahead at or
    /// past it reports [`EOF`]. Normally the index of the `EOF` entry; inside
    /// an interpolation that a line terminator ended, the first token after
    /// that line terminator ([`Parser::enter_interpolation`]).
    limit: usize,
    /// `(InterpStart index, limit)` for every interpolation that the lexer
    /// ended at a line terminator instead of an `InterpEnd` (grammar.md
    /// §4.2), in increasing order of the first index.
    line_ended: Vec<(u32, u32)>,
}

/// One open string or interpolation while [`Parser::new`] replays the
/// lexer's frame stack to find interpolations ended by a line terminator.
#[derive(Clone, Copy)]
enum OpenFrame {
    /// A string literal.
    String,
    /// An interpolation whose `InterpStart` has this index in the
    /// trivia-free view.
    Interp(u32),
}

impl<'t> Parser<'t> {
    /// Builds the parser for `lexed`: the trivia-free token view with the
    /// newline and trivia flags of every token, and the limits of
    /// interpolations that the lexer ended at a line terminator.
    ///
    /// One linear pass over the lexed tokens; it replays the lexer's string
    /// and interpolation frames instead of re-lexing.
    pub(crate) fn new(lexed: &Lexed<'t>, file: FileId) -> Parser<'t> {
        let raw_kinds = lexed.kinds();
        let raw_starts = lexed.starts();
        let text = lexed.source();
        let bytes = text.as_bytes();
        let estimate = raw_kinds.len() / 2 + 1;
        let mut kinds = Vec::with_capacity(estimate);
        let mut starts = Vec::with_capacity(estimate);
        let mut ends = Vec::with_capacity(estimate);
        let mut flags = Vec::with_capacity(estimate);
        // The start of the file counts as a fresh line.
        let mut pending = NEWLINE_BEFORE;
        // The lexer's string and interpolation frames, replayed: a `Newline`
        // token while a string is open is where the lexer ended that string
        // and everything opened inside it.
        let mut open: Vec<OpenFrame> = Vec::new();
        let mut open_strings = 0u32;
        // While a string is open, the index in `open` of the outermost one
        // (frames below it outlive it, so the index needs no search).
        let mut lowest_string = 0usize;
        let mut line_ended: Vec<(u32, u32)> = Vec::new();
        // Entries of `line_ended` still waiting for their limit: the index
        // of the next non-trivia token.
        let mut unresolved = 0usize;
        // `starts` has one sentinel entry more than `kinds`, so the windows
        // pair every token with its start and end.
        for (&kind, range) in raw_kinds.iter().zip(raw_starts.windows(2)) {
            let (start, end) = (range[0], range[1]);
            match kind {
                SyntaxKind::Newline => {
                    pending |= NEWLINE_BEFORE | TRIVIA_BEFORE;
                    if open_strings > 0 {
                        let lowest = lowest_string.min(open.len());
                        for frame in &open[lowest..] {
                            if let OpenFrame::Interp(index) = *frame {
                                line_ended.push((index, u32::MAX));
                                unresolved += 1;
                            }
                        }
                        open.truncate(lowest);
                        open_strings = 0;
                    }
                }
                SyntaxKind::BlockComment => {
                    pending |= TRIVIA_BEFORE;
                    let body = bytes.get(start as usize..end as usize).unwrap_or(&[]);
                    if body.iter().any(|&b| b == b'\n' || b == b'\r') {
                        pending |= NEWLINE_BEFORE;
                    }
                }
                k if k.is_trivia() => pending |= TRIVIA_BEFORE,
                _ => {
                    let index = u32::try_from(kinds.len()).unwrap_or(u32::MAX);
                    if unresolved > 0 {
                        resolve(&mut line_ended, &mut unresolved, index);
                    }
                    match kind {
                        SyntaxKind::StringStart => {
                            if open_strings == 0 {
                                lowest_string = open.len();
                            }
                            open.push(OpenFrame::String);
                            open_strings += 1;
                        }
                        SyntaxKind::InterpStart => open.push(OpenFrame::Interp(index)),
                        SyntaxKind::StringEnd => {
                            if matches!(open.last(), Some(OpenFrame::String)) {
                                open.pop();
                                open_strings -= 1;
                            }
                        }
                        SyntaxKind::InterpEnd => {
                            if matches!(open.last(), Some(OpenFrame::Interp(_))) {
                                open.pop();
                            }
                        }
                        _ => {}
                    }
                    kinds.push(kind);
                    starts.push(start);
                    ends.push(end);
                    flags.push(pending);
                    pending = 0;
                }
            }
        }
        let len = u32::try_from(text.len()).unwrap_or(u32::MAX);
        if unresolved > 0 {
            let index = u32::try_from(kinds.len()).unwrap_or(u32::MAX);
            resolve(&mut line_ended, &mut unresolved, index);
        }
        let limit = kinds.len();
        kinds.push(EOF);
        starts.push(len);
        ends.push(len);
        flags.push(pending);
        Parser {
            text,
            file,
            kinds,
            starts,
            ends,
            flags,
            pos: 0,
            events: Vec::with_capacity(estimate * 2),
            diagnostics: Vec::new(),
            fuel: std::cell::Cell::new(FUEL),
            newlines: true,
            no_trailing_lambda: false,
            depth: 0,
            line_cache: std::cell::Cell::new((0, 0)),
            skipped_to_eof_from: None,
            limit,
            line_ended,
        }
    }

    /// Consumes the parser and returns its events and diagnostics.
    pub(crate) fn finish(self) -> (Vec<Event>, Vec<Diagnostic>) {
        (self.events, self.diagnostics)
    }

    // ------------------------------------------------------------- markers

    /// Starts a node at the current token.
    pub(crate) fn start(&mut self) -> Marker {
        let first = u32::try_from(self.pos).unwrap_or(u32::MAX);
        self.start_at(first)
    }

    /// Starts a node whose first token is `first_token` by pushing a
    /// tombstone `Start` event.
    fn start_at(&mut self, first_token: u32) -> Marker {
        let pos = u32::try_from(self.events.len()).unwrap_or(u32::MAX);
        self.events.push(Event::Start {
            kind: TOMBSTONE,
            forward_parent: None,
        });
        Marker { pos, first_token }
    }

    // --------------------------------------------------------- token access

    /// Returns the kind of the token `n` positions ahead (0 is the current
    /// token), or [`EOF`].
    pub(crate) fn nth(&self, n: usize) -> SyntaxKind {
        let fuel = self.fuel.get();
        if fuel == 0 {
            debug_assert!(false, "the parser is stuck at token {}", self.pos);
            return EOF;
        }
        self.fuel.set(fuel - 1);
        let index = self.pos + n;
        if index >= self.limit {
            return EOF;
        }
        self.kinds.get(index).copied().unwrap_or(EOF)
    }

    /// Returns the kind of the token `n` positions ahead, like
    /// [`Parser::nth`] but without burning fuel. Only for scans that end by
    /// construction (at a delimiter or the end of input) and may look
    /// further ahead than the fuel allows, such as the lambda-parameter
    /// lookahead.
    pub(crate) fn peek(&self, n: usize) -> SyntaxKind {
        let index = self.pos + n;
        if index >= self.limit {
            return EOF;
        }
        self.kinds.get(index).copied().unwrap_or(EOF)
    }

    /// Returns the kind of the token at `index` of the trivia-free view,
    /// without burning fuel (for looking back at consumed tokens).
    pub(crate) fn kind_at(&self, index: usize) -> SyntaxKind {
        self.kinds.get(index).copied().unwrap_or(EOF)
    }

    /// Returns the kind of the current token.
    pub(crate) fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    /// Returns `true` if the current token has kind `kind`.
    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    /// Returns `true` at the end of the input.
    pub(crate) fn at_eof(&self) -> bool {
        self.current() == EOF
    }

    /// Returns the text of the token `n` positions ahead ("" past the end).
    pub(crate) fn nth_text(&self, n: usize) -> &'t str {
        let index = self.pos + n;
        match (self.starts.get(index), self.ends.get(index)) {
            (Some(&s), Some(&e)) => self.text.get(s as usize..e as usize).unwrap_or(""),
            _ => "",
        }
    }

    /// Returns `true` if the token `n` ahead is an identifier spelled `word`.
    pub(crate) fn nth_is_word(&self, n: usize, word: &str) -> bool {
        self.nth(n) == SyntaxKind::Ident && self.nth_text(n) == word
    }

    /// Returns `true` if the current token is an identifier spelled `word`.
    pub(crate) fn at_word(&self, word: &str) -> bool {
        self.nth_is_word(0, word)
    }

    /// Returns `true` if the token `n` ahead is preceded by a line terminator,
    /// whether or not newlines are significant here.
    pub(crate) fn nth_on_new_line(&self, n: usize) -> bool {
        self.flags
            .get(self.pos + n)
            .is_some_and(|f| f & NEWLINE_BEFORE != 0)
    }

    /// Returns `true` if the current token starts a new line.
    pub(crate) fn on_new_line(&self) -> bool {
        self.nth_on_new_line(0)
    }

    /// Returns `true` if a *significant* newline precedes the current token:
    /// one that can end the current construct (rules N1 and N3).
    pub(crate) fn nl(&self) -> bool {
        self.newlines && self.on_new_line()
    }

    /// Returns `true` if the token `n` ahead directly follows the previous
    /// token, with no trivia in between.
    pub(crate) fn nth_joined(&self, n: usize) -> bool {
        self.flags
            .get(self.pos + n)
            .is_some_and(|f| f & TRIVIA_BEFORE == 0)
    }

    /// Returns `true` if the current token directly follows the previous one.
    pub(crate) fn joined(&self) -> bool {
        self.nth_joined(0)
    }

    /// Consumes the current token with its own kind.
    pub(crate) fn bump(&mut self) {
        let kind = self.kinds.get(self.pos).copied().unwrap_or(EOF);
        self.bump_remap(kind);
    }

    /// Consumes the current token, giving it `kind` in the tree.
    pub(crate) fn bump_remap(&mut self, kind: SyntaxKind) {
        if self.pos >= self.limit {
            // At EOF (or the limit of an interpolation ended by a line
            // terminator): nothing to consume.
            return;
        }
        self.events.push(Event::Token { kind });
        self.pos += 1;
        self.fuel.set(FUEL);
    }

    /// Consumes the current token if it has kind `kind`.
    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    // ------------------------------------------------------------- offsets

    /// Returns the source text being parsed.
    pub(crate) fn text(&self) -> &'t str {
        self.text
    }

    /// Returns the start offset of token `index` in the trivia-free view, or
    /// 0 if there is no such token.
    pub(crate) fn token_start(&self, index: usize) -> u32 {
        self.starts.get(index).copied().unwrap_or(0)
    }

    /// Returns the end offset of token `index` in the trivia-free view, or 0
    /// if there is no such token.
    pub(crate) fn token_end(&self, index: usize) -> u32 {
        self.ends.get(index).copied().unwrap_or(0)
    }

    /// Returns the index of the current token in the trivia-free view.
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// Returns the byte range of the token `n` ahead.
    pub(crate) fn nth_range(&self, n: usize) -> TextRange {
        let index = self.pos + n;
        TextRange::new(
            TextSize::from(self.token_start(index)),
            TextSize::from(self.token_end(index).max(self.token_start(index))),
        )
    }

    /// Returns the range that identifies the element starting at the current
    /// token: the token itself, or a whole one-line string literal when the
    /// element is a string (K0102 points at `"b"`, not just its quote).
    pub(crate) fn first_element_range(&self) -> TextRange {
        let first = self.current_range();
        if self.current() != SyntaxKind::StringStart {
            return first;
        }
        let mut depth = 0usize;
        let mut index = self.pos;
        let mut end = first.end();
        while index < self.kinds.len() {
            if index > self.pos && self.flags[index] & NEWLINE_BEFORE != 0 {
                break;
            }
            end = TextSize::from(self.token_end(index));
            match self.kinds[index] {
                SyntaxKind::StringStart => depth += 1,
                SyntaxKind::StringEnd => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                EOF => break,
                _ => {}
            }
            index += 1;
        }
        TextRange::new(first.start(), end.max(first.end()))
    }

    /// Returns the byte range of the current token.
    pub(crate) fn current_range(&self) -> TextRange {
        self.nth_range(0)
    }

    /// Returns the end offset of the previous non-trivia token (0 at the
    /// start of the file).
    pub(crate) fn prev_end(&self) -> u32 {
        match self.pos.checked_sub(1) {
            Some(index) => self.token_end(index),
            None => 0,
        }
    }

    /// Returns the byte range of the previous non-trivia token.
    pub(crate) fn prev_range(&self) -> TextRange {
        match self.pos.checked_sub(1) {
            Some(index) => TextRange::new(
                TextSize::from(self.token_start(index)),
                TextSize::from(self.token_end(index)),
            ),
            None => TextRange::empty(TextSize::from(0)),
        }
    }

    /// Returns `range` as a span in the parsed file.
    pub(crate) fn span(&self, range: TextRange) -> Span {
        Span::new(self.file, range)
    }

    /// The span diagnostics use for something missing at the current
    /// position: the current token if it is on the same line as the previous
    /// one, otherwise the gap right after the previous token
    /// (`docs/compiler/diagnostics.md`, rule 4 and K0100).
    pub(crate) fn expected_span(&self) -> Span {
        if self.at_eof() || (self.on_new_line() && self.pos > 0) {
            self.span(TextRange::empty(TextSize::from(self.prev_end())))
        } else {
            self.span(self.current_range())
        }
    }

    /// Describes what was found for an "expected X, found Y" diagnostic at
    /// [`Parser::expected_span`]: "end of line" when that span is the gap
    /// at the end of the previous line, otherwise [`Parser::found_token`].
    pub(crate) fn found(&self) -> String {
        if !self.at_eof() && self.on_new_line() && self.pos > 0 {
            return "end of line".to_owned();
        }
        self.found_token()
    }

    /// Describes the current token itself, for a diagnostic whose span is
    /// that token (a junk run is reported at its first token, even when it
    /// starts a line: "expected a member, found `+`").
    pub(crate) fn found_token(&self) -> String {
        if self.at_eof() {
            if self.pos + 1 < self.kinds.len() {
                // The limit of an interpolation ended by a line terminator.
                return "end of line".to_owned();
            }
            return "end of file".to_owned();
        }
        let text = self.nth_text(0);
        let mut out = String::from("`");
        for (index, c) in text.chars().enumerate() {
            if index == 24 {
                out.push('…');
                break;
            }
            if c.is_control() {
                out.push_str(&format!("\\u{{{:x}}}", u32::from(c)));
            } else {
                out.push(c);
            }
        }
        out.push('`');
        out
    }

    /// Returns the indentation (leading spaces and tabs) of the line that
    /// contains `offset`.
    pub(crate) fn indentation_at(&self, offset: u32) -> &'t str {
        let bytes = self.text.as_bytes();
        let offset = (offset as usize).min(bytes.len());
        let is_eol = |b: &u8| *b == b'\n' || *b == b'\r';
        // Diagnostics are mostly reported in increasing offset order, so the
        // line start found last time is reused and only the text since then
        // is scanned; this keeps many fixes on one long line linear.
        let (cached_start, cached_until) = self.line_cache.get();
        let line_start = if offset >= cached_until {
            match bytes[cached_until..offset].iter().rposition(is_eol) {
                Some(i) => cached_until + i + 1,
                None => cached_start,
            }
        } else {
            bytes[..offset]
                .iter()
                .rposition(is_eol)
                .map_or(0, |i| i + 1)
        };
        self.line_cache.set((line_start, offset));
        let mut end = line_start;
        while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
            end += 1;
        }
        self.text.get(line_start..end).unwrap_or("")
    }

    /// Returns the range of the whitespace directly before the current token
    /// (empty if there is none), stopping at line terminators.
    pub(crate) fn whitespace_before(&self) -> TextRange {
        let start = self.token_start(self.pos) as usize;
        let bytes = self.text.as_bytes();
        let mut begin = start.min(bytes.len());
        while begin > 0 && (bytes[begin - 1] == b' ' || bytes[begin - 1] == b'\t') {
            begin -= 1;
        }
        TextRange::new(
            TextSize::from(u32::try_from(begin).unwrap_or(0)),
            TextSize::from(u32::try_from(start).unwrap_or(0)),
        )
    }

    /// Returns the range of the whitespace directly after the byte `offset`.
    pub(crate) fn whitespace_after(&self, offset: u32) -> TextRange {
        let bytes = self.text.as_bytes();
        let start = (offset as usize).min(bytes.len());
        let mut end = start;
        while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
            end += 1;
        }
        TextRange::new(
            TextSize::from(u32::try_from(start).unwrap_or(0)),
            TextSize::from(u32::try_from(end).unwrap_or(0)),
        )
    }

    // --------------------------------------------------------- diagnostics

    /// Records a diagnostic. Returns `false` (dropping it) if the diagnostic
    /// just recorded has the same code and span: an enclosing construct
    /// that describes the same mistake again (`enum<` is "expected a name"
    /// at the `<`, and the generic list that follows would add "expected a
    /// generic parameter name" there) adds only noise. The first, innermost
    /// description is kept (docs/compiler/diagnostics.md, de-duplication).
    pub(crate) fn report(&mut self, diagnostic: Diagnostic) -> bool {
        if self
            .diagnostics
            .last()
            .is_some_and(|last| last.code == diagnostic.code && last.span == diagnostic.span)
        {
            return false;
        }
        self.diagnostics.push(diagnostic);
        true
    }

    /// Reports `code` ("expected {what}") at [`Parser::expected_span`].
    ///
    /// A second "expected ..." diagnostic at exactly the same span as the
    /// previous one is dropped: it describes the same gap from the point of
    /// view of an enclosing construct and would only add noise.
    pub(crate) fn expected(&mut self, code: k_diagnostics::Code, what: &str) {
        let span = self.expected_span();
        let duplicate = self.diagnostics.last().is_some_and(|last| {
            last.span == span
                && matches!(
                    last.code,
                    codes::EXPECTED_TOKEN
                        | codes::EXPECTED_EXPRESSION
                        | codes::EXPECTED_TYPE
                        | codes::EXPECTED_PATTERN
                )
        });
        if duplicate {
            return;
        }
        let message = format!("expected {what}, found {}", self.found());
        self.report(Diagnostic::new(code, span, message));
    }

    /// Reports the start of a junk run (K0122, K0125 or K0126 at its first
    /// token), unless the diagnostic just reported has exactly the same span:
    /// that one already describes this token (`do = 1` reports K0100 at the
    /// `=`, which is then also the first token of a junk run). One gap gets
    /// one diagnostic.
    pub(crate) fn report_junk(&mut self, diagnostic: Diagnostic) {
        if self
            .diagnostics
            .last()
            .is_some_and(|last| last.span == diagnostic.span)
        {
            return;
        }
        self.report(diagnostic);
    }

    /// Reports K0100 "expected {what}".
    pub(crate) fn expected_token(&mut self, what: &str) {
        self.expected(codes::EXPECTED_TOKEN, what);
    }

    /// Consumes `kind` or reports K0100 "expected {what}".
    pub(crate) fn expect(&mut self, kind: SyntaxKind, what: &str) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.eat_error_tokens();
        if self.eat(kind) {
            return true;
        }
        self.expected_token(what);
        false
    }

    /// Reports K0101 for the opening delimiter at `open`.
    pub(crate) fn unclosed(&mut self, open: TextRange) {
        if self
            .skipped_to_eof_from
            .is_some_and(|from| open.start() < from)
        {
            return;
        }
        let text = self
            .text
            .get(usize::from(open.start())..usize::from(open.end()))
            .unwrap_or("(");
        let close = match text {
            "(" => ")",
            "[" => "]",
            "<" => ">",
            _ => "}",
        };
        let span = self.span(open);
        self.report(
            Diagnostic::new(
                codes::UNCLOSED_DELIMITER,
                span,
                format!("unclosed delimiter `{text}`"),
            )
            .with_help(format!("add the matching `{close}`")),
        );
    }

    /// Wraps a run of lexer [`SyntaxKind::Error`] tokens in an `ErrorNode`
    /// without a parser diagnostic (the lexer already reported K0001).
    /// Returns `true` if there was such a run.
    pub(crate) fn eat_error_tokens(&mut self) -> bool {
        if !self.at(SyntaxKind::Error) {
            return false;
        }
        let m = self.start();
        while self.at(SyntaxKind::Error) {
            self.bump();
        }
        m.complete(self, SyntaxKind::ErrorNode);
        true
    }

    /// Consumes the current token, or a whole balanced group if it opens
    /// one, without recursion. A string literal, interpolations included, is
    /// one group too, so a keyword inside `"{func f() {}}"` is never taken
    /// for the start of a declaration.
    pub(crate) fn bump_balanced(&mut self) {
        let mut depth: usize = 0;
        loop {
            match self.current() {
                EOF => return,
                SyntaxKind::StringStart => {
                    self.bump_string();
                    if depth == 0 {
                        return;
                    }
                    continue;
                }
                SyntaxKind::LParen | SyntaxKind::LBrack | SyntaxKind::LBrace => depth += 1,
                SyntaxKind::RParen | SyntaxKind::RBrack | SyntaxKind::RBrace => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            self.bump();
            if depth == 0 {
                return;
            }
        }
    }

    /// Consumes a whole string literal starting at the current
    /// `StringStart`: up to its matching `StringEnd`, or up to the line
    /// terminator that ended it (the lexer reported K0002).
    fn bump_string(&mut self) {
        let mut depth: usize = 0;
        let mut first = true;
        loop {
            if !first && self.on_new_line() {
                return;
            }
            first = false;
            match self.current() {
                EOF => return,
                SyntaxKind::StringStart => depth += 1,
                SyntaxKind::StringEnd => {
                    self.bump();
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return;
                    }
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
    }

    /// Wraps tokens in an `ErrorNode` until `stop` holds (or the end of
    /// file), skipping balanced groups as a whole. Consumes at least one
    /// token unless already stopped.
    pub(crate) fn skip_until(&mut self, stop: impl Fn(&Parser<'t>) -> bool) {
        let m = self.start();
        let mut first = true;
        while !self.at_eof() && (first || !stop(self)) {
            first = false;
            self.bump_balanced();
        }
        m.complete(self, SyntaxKind::ErrorNode);
    }

    // ------------------------------------------------------ interpolations

    /// Called right after the `InterpStart` at index `open` was consumed. If
    /// the lexer ended that interpolation at a line terminator (its string
    /// ended there; grammar.md §4.2), the parser's view ends there too: the
    /// tokens after the line terminator look like the end of file until
    /// [`Parser::leave_interpolation`], so `(`, `[` and `{` opened inside
    /// are unclosed (K0101) instead of swallowing the following lines.
    /// Returns the limit to restore.
    pub(crate) fn enter_interpolation(&mut self, open: usize) -> usize {
        let saved = self.limit;
        let Ok(open) = u32::try_from(open) else {
            return saved;
        };
        if let Ok(found) = self.line_ended.binary_search_by_key(&open, |&(at, _)| at) {
            let end = self.line_ended[found].1 as usize;
            self.limit = self.limit.min(end);
        }
        saved
    }

    /// Restores the limit returned by [`Parser::enter_interpolation`].
    pub(crate) fn leave_interpolation(&mut self, saved: usize) {
        self.limit = saved;
    }

    // ------------------------------------------------------------- nesting

    /// Enters a recursive construct. Returns `false` (after reporting K0134
    /// once and skipping the rest of the nested input) when the nesting
    /// limit is reached; the caller must then return without recursing.
    pub(crate) fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            self.too_deep();
            return false;
        }
        self.depth += 1;
        true
    }

    /// Leaves a construct entered with [`Parser::enter`].
    ///
    /// Unwinding many nested constructs at the end of the file performs a
    /// few lookaheads per level without consuming a token; leaving a level
    /// refills the fuel so that is not mistaken for a stuck loop (a real
    /// stuck loop never leaves its level).
    pub(crate) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        self.fuel.set(FUEL);
    }

    /// Reports K0134 and skips the remainder of the over-deep construct: all
    /// tokens up to the first closing delimiter that is not matched inside
    /// the skipped text (which belongs to an enclosing construct).
    fn too_deep(&mut self) {
        let span = self.span(self.current_range());
        let already = self
            .diagnostics
            .last()
            .is_some_and(|d| d.code == codes::NESTING_TOO_DEEP);
        if !already {
            self.report(
                Diagnostic::new(
                    codes::NESTING_TOO_DEEP,
                    span,
                    format!("nesting exceeds the limit of {MAX_DEPTH} levels"),
                )
                .with_help("split the expression or declaration into smaller parts"),
            );
        }
        let m = self.start();
        let mut depth: usize = 0;
        let from = span.start();
        loop {
            match self.current() {
                EOF => {
                    self.skipped_to_eof_from.get_or_insert(from);
                    break;
                }
                SyntaxKind::LParen
                | SyntaxKind::LBrack
                | SyntaxKind::LBrace
                | SyntaxKind::InterpStart => depth += 1,
                SyntaxKind::RParen
                | SyntaxKind::RBrack
                | SyntaxKind::RBrace
                | SyntaxKind::InterpEnd => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
            self.bump();
        }
        m.complete(self, SyntaxKind::ErrorNode);
    }

    // ------------------------------------------------------- fix helpers

    /// Returns a machine-applicable fix that replaces `range` with `text`.
    pub(crate) fn fix(
        &self,
        message: impl Into<String>,
        range: TextRange,
        text: impl Into<String>,
    ) -> Fix {
        Fix::replace(message, self.span(range), text)
    }
}

impl Marker {
    /// Completes the node with `kind`.
    pub(crate) fn complete(self, p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        if let Some(Event::Start { kind: slot, .. }) = p.events.get_mut(self.pos as usize) {
            *slot = kind;
        }
        p.events.push(Event::Finish);
        CompletedMarker {
            pos: self.pos,
            kind,
            first_token: self.first_token,
        }
    }

    /// Drops the node: its children become children of the enclosing node.
    pub(crate) fn abandon(self, p: &mut Parser<'_>) {
        let index = self.pos as usize;
        if index + 1 == p.events.len()
            && matches!(
                p.events.last(),
                Some(Event::Start {
                    kind: TOMBSTONE,
                    forward_parent: None
                })
            )
        {
            p.events.pop();
        }
    }
}

/// Gives the last `unresolved` entries of `line_ended` the limit `index`.
fn resolve(line_ended: &mut [(u32, u32)], unresolved: &mut usize, index: u32) {
    let from = line_ended.len() - *unresolved;
    for entry in &mut line_ended[from..] {
        entry.1 = index;
    }
    *unresolved = 0;
}

/// Returns `true` for the `Start` kind of an abandoned marker.
pub(crate) fn is_tombstone(kind: SyntaxKind) -> bool {
    kind == TOMBSTONE
}
