//! The K lexer: source text to a flat, lossless token stream.
//!
//! [`lex`] turns a source text into a [`Lexed`] token stream. The stream is
//! *lossless*: every byte of the input belongs to exactly one token, trivia
//! (whitespace, newlines, comments) included, so concatenating the token
//! texts reproduces the input exactly. Problems such as an unterminated
//! string are reported as diagnostics alongside the tokens; lexing never
//! fails and never panics.
//!
//! The token kinds, the lexer modes (K code, strings and interpolations,
//! style blocks) and the exact tokenization rules are specified in
//! `docs/compiler/grammar.md`, sections "Lexical structure" and "Lexer
//! modes". The diagnostics and their spans are specified in
//! `docs/compiler/diagnostics.md`, "Lexer codes".
//!
//! # Representation
//!
//! Tokens are stored as two parallel arrays (kinds and start offsets) rather
//! than a vector of structs, which keeps the hot loop of the parser
//! cache-friendly. Token texts are slices of the original source; nothing is
//! copied.
//!
//! # Algorithm
//!
//! The lexer is a single forward pass over the bytes of the source. It keeps
//! an explicit stack of *frames* (strings, interpolations and style blocks)
//! on the heap instead of recursing, so arbitrarily deep nesting such as
//! `"{"{"{...}"}"}"` runs in constant native stack space. Each token is
//! classified by its first byte; identifiers, numbers and string text are
//! scanned with tight byte loops. The only lookahead beyond a few bytes is
//! the scan after the `style` keyword (past trivia, to find a `{`) and the
//! scan for the `}` of a `\u{...}` escape, both of which are bounded so the
//! whole pass stays linear in the input size.
//!
//! # Limits
//!
//! Offsets are `u32`, like [`TextSize`]. Sources longer than `u32::MAX`
//! bytes are not supported by the toolchain; the driver rejects them before
//! lexing. If such a text reaches the lexer anyway, offsets saturate and the
//! stream stays well-formed but is no longer lossless.

use k_diagnostics::{Diagnostic, FileId, Fix, Span, TextRange, TextSize, codes};

use crate::kind::SyntaxKind;

/// The result of lexing one source text.
///
/// Token `i` has kind [`Lexed::kind`]`(i)` and covers
/// [`Lexed::range`]`(i)` of the source.
#[derive(Clone, Debug)]
pub struct Lexed<'a> {
    text: &'a str,
    kinds: Vec<SyntaxKind>,
    /// Start offset of every token, followed by one sentinel entry equal to
    /// the length of the text, so `starts.len() == kinds.len() + 1`.
    starts: Vec<u32>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexed<'a> {
    /// Returns the source text that was lexed.
    #[must_use]
    pub fn source(&self) -> &'a str {
        self.text
    }

    /// Returns the number of tokens.
    #[must_use]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Returns `true` if there are no tokens, which happens exactly when the
    /// source is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Returns the kind of token `index`, or `None` past the end.
    #[must_use]
    pub fn kind(&self, index: usize) -> Option<SyntaxKind> {
        self.kinds.get(index).copied()
    }

    /// Returns all token kinds in order.
    #[must_use]
    pub fn kinds(&self) -> &[SyntaxKind] {
        &self.kinds
    }

    /// Returns the start offset of every token followed by one sentinel
    /// entry equal to the end of the last token, so the result has
    /// [`Lexed::len`]` + 1` entries. Token `i` covers
    /// `starts[i]..starts[i + 1]`.
    #[must_use]
    pub fn starts(&self) -> &[u32] {
        &self.starts
    }

    /// Returns the byte range of token `index`, or `None` past the end.
    #[must_use]
    pub fn range(&self, index: usize) -> Option<TextRange> {
        if index >= self.kinds.len() {
            return None;
        }
        let start = *self.starts.get(index)?;
        let end = *self.starts.get(index + 1)?;
        Some(TextRange::new(TextSize::from(start), TextSize::from(end)))
    }

    /// Returns the text of token `index`, or `None` past the end.
    #[must_use]
    pub fn text(&self, index: usize) -> Option<&'a str> {
        let range = self.range(index)?;
        self.text
            .get(usize::from(range.start())..usize::from(range.end()))
    }

    /// Iterates over `(kind, text)` pairs of all tokens.
    pub fn iter(&self) -> impl Iterator<Item = (SyntaxKind, &'a str)> + '_ {
        (0..self.len()).map(|i| (self.kinds[i], self.text(i).unwrap_or("")))
    }

    /// Returns the diagnostics reported while lexing, sorted by position
    /// (start offset, then end offset, then code).
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consumes the token stream and returns its diagnostics.
    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

/// Builds a [`Lexed`] token by token. Used by tests and tools that need
/// hand-made token streams; the lexer itself writes its arrays directly.
#[derive(Debug)]
pub struct LexedBuilder<'a> {
    lexed: Lexed<'a>,
    offset: u32,
}

impl<'a> LexedBuilder<'a> {
    /// Starts building the token stream for `text`.
    #[must_use]
    pub fn new(text: &'a str) -> LexedBuilder<'a> {
        LexedBuilder {
            lexed: Lexed {
                text,
                kinds: Vec::with_capacity(text.len() / 4),
                starts: Vec::with_capacity(text.len() / 4 + 1),
                diagnostics: Vec::new(),
            },
            offset: 0,
        }
    }

    /// Appends a token of `kind` that is `len` bytes long. Zero-length tokens
    /// are ignored, because every token must cover at least one byte.
    pub fn push(&mut self, kind: SyntaxKind, len: u32) {
        if len == 0 {
            return;
        }
        self.lexed.kinds.push(kind);
        self.lexed.starts.push(self.offset);
        self.offset = self.offset.saturating_add(len);
    }

    /// Records a diagnostic.
    pub fn diagnostic(&mut self, diagnostic: Diagnostic) {
        self.lexed.diagnostics.push(diagnostic);
    }

    /// Finishes the stream. If the pushed tokens do not cover the whole text,
    /// the remainder becomes one [`SyntaxKind::Error`] token, so the result is
    /// always lossless.
    #[must_use]
    pub fn finish(mut self) -> Lexed<'a> {
        let total = u32::try_from(self.lexed.text.len()).unwrap_or(u32::MAX);
        if self.offset < total {
            let rest = total - self.offset;
            self.push(SyntaxKind::Error, rest);
        }
        self.lexed.starts.push(total.min(self.offset));
        self.lexed
    }
}

/// Lexes `text`, which belongs to `file`, into a lossless token stream.
///
/// Diagnostics carry spans in `file` and are sorted by position. See the
/// [module documentation](self) for the guarantees.
///
/// ```
/// use k_diagnostics::FileId;
/// use k_syntax::{lex, SyntaxKind};
///
/// let lexed = lex(FileId::new(0), "let x = 16px");
/// let kinds: Vec<_> = lexed.kinds().iter().copied()
///     .filter(|k| *k != SyntaxKind::Whitespace)
///     .collect();
/// assert_eq!(kinds, [SyntaxKind::LetKw, SyntaxKind::Ident, SyntaxKind::Eq, SyntaxKind::UnitLit]);
/// assert!(lexed.diagnostics().is_empty());
/// ```
#[must_use]
pub fn lex(file: FileId, text: &str) -> Lexed<'_> {
    Lexer::new(file, text).run()
}

// ---------------------------------------------------------------- internals

/// One entry of the mode stack. The bottom of the stack (an empty stack) is
/// code mode.
#[derive(Clone, Copy, Debug)]
enum Frame {
    /// Inside a string literal whose opening `"` is at `open`.
    String { open: u32 },
    /// Inside an interpolation, in a string or in a style value. `depth`
    /// counts unmatched `{` opened inside it. An interpolation in a string
    /// ends at a line terminator together with its string
    /// ([`Lexer::end_line_in_string`]); a style-value interpolation may span
    /// lines. `ends_statement` is set for an interpolation opened in a style
    /// declaration before its `:` (the element rule `button { ... }` of
    /// RA-8): when it closes, the style statement ends too.
    Interp { depth: u32, ends_statement: bool },
    /// Inside a style block. `depth` counts nested blocks.
    Style { depth: u32, state: StyleState },
}

/// The statement state of a style frame (grammar.md §4.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StyleState {
    /// At the start of a statement: the next significant character decides.
    Start,
    /// In a declaration before its `:` (`padding`): `{` opens an
    /// interpolation that also ends the statement when it closes.
    Property,
    /// In a declaration after its `:` (`padding: .md;`): `{` opens an
    /// interpolation.
    Declaration,
    /// In a selector or at-rule prelude: `{` opens a nested block.
    Selector,
}

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    file: FileId,
    kinds: Vec<SyntaxKind>,
    starts: Vec<u32>,
    diagnostics: Vec<Diagnostic>,
    frames: Vec<Frame>,
    /// The number of [`Frame::String`] entries in `frames`. While it is
    /// non-zero, a line terminator in any mode ends the strings.
    open_strings: u32,
    /// While `open_strings` is non-zero, the index in `frames` of the lowest
    /// (outermost) open string. Frames below it cannot be popped while it is
    /// open, so the index stays valid without searching the stack.
    lowest_string: usize,
    /// The kind of the last non-trivia token, for the style-mode rule.
    last_significant: SyntaxKind,
    /// Set after a `style` keyword whose next significant character is `{`.
    pending_style: bool,
    /// Set when a diagnostic was pushed out of position order.
    unsorted: bool,
    /// A position before which a `\u{` escape scan is known to find no `}`
    /// (the scan stopped at a quote or line end there). Keeps pathological
    /// inputs like `"\u{\u{\u{...` linear.
    unicode_scan_fail_until: usize,
}

/// Converts a byte offset to a `u32`, saturating.
fn off(pos: usize) -> u32 {
    u32::try_from(pos).unwrap_or(u32::MAX)
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The length in bytes of the UTF-8 sequence starting with `b`.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Recognizes a hard keyword from the bytes of an identifier.
///
/// Dispatches on the first byte so each identifier is compared against at
/// most a handful of candidates.
fn keyword(word: &[u8]) -> Option<SyntaxKind> {
    use SyntaxKind as K;
    let kind = match word.first()? {
        b'a' => match word {
            b"and" => K::AndKw,
            b"as" => K::AsKw,
            b"async" => K::AsyncKw,
            b"await" => K::AwaitKw,
            _ => return None,
        },
        b'b' => match word {
            b"bind" => K::BindKw,
            b"break" => K::BreakKw,
            _ => return None,
        },
        b'c' => match word {
            b"case" => K::CaseKw,
            b"catch" => K::CatchKw,
            b"class" => K::ClassKw,
            b"continue" => K::ContinueKw,
            _ => return None,
        },
        b'd' => match word {
            b"default" => K::DefaultKw,
            b"do" => K::DoKw,
            _ => return None,
        },
        b'e' => match word {
            b"else" => K::ElseKw,
            b"enum" => K::EnumKw,
            b"expect" => K::ExpectKw,
            b"extend" => K::ExtendKw,
            _ => return None,
        },
        b'f' => match word {
            b"false" => K::FalseKw,
            b"for" => K::ForKw,
            b"func" => K::FuncKw,
            _ => return None,
        },
        b'g' => match word {
            b"guard" => K::GuardKw,
            _ => return None,
        },
        b'i' => match word {
            b"if" => K::IfKw,
            b"implements" => K::ImplementsKw,
            b"import" => K::ImportKw,
            b"in" => K::InKw,
            b"init" => K::InitKw,
            b"interface" => K::InterfaceKw,
            b"is" => K::IsKw,
            _ => return None,
        },
        b'l' => match word {
            b"let" => K::LetKw,
            _ => return None,
        },
        b'm' => match word {
            b"match" => K::MatchKw,
            _ => return None,
        },
        b'n' => match word {
            b"nil" => K::NilKw,
            b"not" => K::NotKw,
            _ => return None,
        },
        b'o' => match word {
            b"or" => K::OrKw,
            _ => return None,
        },
        b'p' => match word {
            b"private" => K::PrivateKw,
            b"public" => K::PublicKw,
            _ => return None,
        },
        b'r' => match word {
            b"requires" => K::RequiresKw,
            b"return" => K::ReturnKw,
            b"rpc" => K::RpcKw,
            _ => return None,
        },
        b's' => match word {
            b"self" => K::SelfKw,
            b"static" => K::StaticKw,
            b"struct" => K::StructKw,
            b"style" => K::StyleKw,
            _ => return None,
        },
        b't' => match word {
            b"throw" => K::ThrowKw,
            b"throws" => K::ThrowsKw,
            b"true" => K::TrueKw,
            b"try" => K::TryKw,
            _ => return None,
        },
        b'u' => match word {
            b"use" => K::UseKw,
            _ => return None,
        },
        b'v' => match word {
            b"var" => K::VarKw,
            _ => return None,
        },
        b'w' => match word {
            b"when" => K::WhenKw,
            b"where" => K::WhereKw,
            b"while" => K::WhileKw,
            _ => return None,
        },
        _ => return None,
    };
    Some(kind)
}

/// Returns `true` if `b` (the first byte of a character) can start a token
/// in code mode. Non-ASCII characters never can (a BOM at offset 0 is
/// handled before this check).
fn starts_code_token(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'_'
            | b'"'
            | b'#'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b','
            | b':'
            | b';'
            | b'.'
            | b'?'
            | b'@'
            | b'-'
            | b'='
            | b'!'
            | b'<'
            | b'>'
            | b'+'
            | b'*'
            | b'/'
            | b'%'
            | b'&'
            | b'|'
            | b'~'
    )
}

/// Returns `true` if `b` can start a token in style mode (grammar.md §4.4).
fn starts_style_token(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'_'
            | b'"'
            | b'#'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b','
            | b':'
            | b';'
            | b'.'
            | b'&'
            | b'>'
            | b'+'
            | b'~'
            | b'*'
            | b'/'
            | b'!'
            | b'='
            | b'-'
            | b'%'
            | b'@'
    )
}

/// Escapes a piece of source text for a one-line diagnostic message.
fn display_text(text: &str, out: &mut String) {
    const MAX_CHARS: usize = 24;
    for (index, c) in text.chars().enumerate() {
        if index == MAX_CHARS {
            out.push('…');
            return;
        }
        if c.is_control() || c == '\u{feff}' || (c.is_whitespace() && c != ' ') {
            out.push_str(&format!("\\u{{{:x}}}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
}

impl<'a> Lexer<'a> {
    fn new(file: FileId, text: &'a str) -> Lexer<'a> {
        // A typical K file averages about four bytes per token.
        let estimate = text.len() / 4 + 16;
        Lexer {
            text,
            bytes: text.as_bytes(),
            pos: 0,
            file,
            kinds: Vec::with_capacity(estimate),
            starts: Vec::with_capacity(estimate + 1),
            diagnostics: Vec::new(),
            frames: Vec::new(),
            open_strings: 0,
            lowest_string: 0,
            last_significant: SyntaxKind::Newline,
            pending_style: false,
            unsorted: false,
            unicode_scan_fail_until: 0,
        }
    }

    fn run(mut self) -> Lexed<'a> {
        let len = self.bytes.len();
        while self.pos < len {
            let start = self.pos;
            if self.open_strings > 0 && matches!(self.bytes[self.pos], b'\n' | b'\r') {
                self.end_line_in_string();
                continue;
            }
            let kind = match self.frames.last().copied() {
                None | Some(Frame::Interp { .. }) => self.code_token(),
                Some(Frame::String { .. }) => match self.string_token() {
                    Some(kind) => kind,
                    None => continue,
                },
                Some(Frame::Style { .. }) => self.style_token(),
            };
            if self.pos == start {
                // Defensive: every branch consumes at least one byte. Should
                // a bug ever violate that, make progress instead of looping.
                self.pos += utf8_len(self.bytes[start]).min(len - start);
                self.push(SyntaxKind::Error, start);
                self.unexpected(start);
                continue;
            }
            self.push(kind, start);
        }
        // End of file: every open frame simply ends; strings report K0002.
        while let Some(frame) = self.frames.pop() {
            if let Frame::String { open } = frame {
                self.unterminated_string(open);
            }
        }
        self.starts.push(off(len));
        let mut diagnostics = self.diagnostics;
        if self.unsorted {
            diagnostics.sort_by_key(|d| (d.span.start(), d.span.end(), d.code));
        }
        Lexed {
            text: self.text,
            kinds: self.kinds,
            starts: self.starts,
            diagnostics,
        }
    }

    // ------------------------------------------------------------ helpers

    fn push(&mut self, kind: SyntaxKind, start: usize) {
        self.kinds.push(kind);
        self.starts.push(off(start));
        if !kind.is_trivia() {
            self.last_significant = kind;
            if kind != SyntaxKind::StyleKw {
                self.pending_style = false;
            }
        }
    }

    fn peek(&self, n: usize) -> u8 {
        self.bytes.get(self.pos + n).copied().unwrap_or(0)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.file,
            TextRange::new(TextSize::from(off(start)), TextSize::from(off(end))),
        )
    }

    fn diagnostic(&mut self, diagnostic: Diagnostic) {
        if let Some(last) = self.diagnostics.last()
            && (last.span.start(), last.span.end())
                > (diagnostic.span.start(), diagnostic.span.end())
        {
            self.unsorted = true;
        }
        self.diagnostics.push(diagnostic);
    }

    fn unterminated_string(&mut self, open: u32) {
        let open = open as usize;
        let span = self.span(open, open + 1);
        self.diagnostic(
            Diagnostic::new(
                codes::UNTERMINATED_STRING,
                span,
                "unterminated string literal",
            )
            .with_help("close the string with `\"` before the end of the line"),
        );
    }

    /// A line terminator inside a string, at any depth of interpolations,
    /// style blocks and style-value interpolations nested in it, ends the
    /// outermost open string and every frame above it (grammar.md §4.2):
    /// a string cannot span lines, so nothing opened inside it can either.
    /// Each ended string reports K0002, in source order.
    fn end_line_in_string(&mut self) {
        if self.open_strings == 0 {
            return;
        }
        let lowest = self.lowest_string.min(self.frames.len());
        for index in lowest..self.frames.len() {
            if let Frame::String { open } = self.frames[index] {
                self.unterminated_string(open);
            }
        }
        self.frames.truncate(lowest);
        self.open_strings = 0;
    }

    /// Reports K0001 for the `Error` token that starts at `start` and ends at
    /// the current position.
    fn unexpected(&mut self, start: usize) {
        let text = self.text.get(start..self.pos).unwrap_or("");
        let mut message = String::with_capacity(32);
        if text.chars().nth(1).is_some() {
            message.push_str("unexpected characters `");
        } else {
            message.push_str("unexpected character `");
        }
        display_text(text, &mut message);
        message.push('`');
        let mut diagnostic = Diagnostic::new(
            codes::UNEXPECTED_CHARACTER,
            self.span(start, self.pos),
            message,
        );
        if text.ends_with('$') && is_ident_start(self.peek(0)) {
            let name_start = self.pos;
            let mut end = name_start;
            while end < self.bytes.len() && is_ident_continue(self.bytes[end]) {
                end += 1;
            }
            let name = self.text.get(name_start..end).unwrap_or("");
            diagnostic = diagnostic.with_help(format!(
                "K has no `$` sigil; to pass a two-way binding, write `bind {name}`"
            ));
        }
        self.diagnostic(diagnostic);
    }

    /// Consumes a maximal run of characters that cannot start a token in the
    /// current mode and reports one K0001 for it.
    fn error_run(&mut self, style: bool) -> SyntaxKind {
        let start = self.pos;
        let len = self.bytes.len();
        loop {
            let b = self.bytes[self.pos];
            self.pos += utf8_len(b).min(len - self.pos);
            if self.pos >= len {
                break;
            }
            let next = self.bytes[self.pos];
            let starts = if style {
                starts_style_token(next)
            } else {
                starts_code_token(next)
            };
            if starts {
                break;
            }
        }
        self.unexpected(start);
        SyntaxKind::Error
    }

    fn eat_while(&mut self, predicate: impl Fn(u8) -> bool) {
        let len = self.bytes.len();
        while self.pos < len && predicate(self.bytes[self.pos]) {
            self.pos += 1;
        }
    }

    // ------------------------------------------------------------- trivia

    /// Lexes whitespace, a newline or a comment at the current position, if
    /// there is one.
    fn trivia(&mut self) -> Option<SyntaxKind> {
        match self.bytes[self.pos] {
            b' ' | b'\t' => {
                self.eat_while(|b| b == b' ' || b == b'\t');
                Some(SyntaxKind::Whitespace)
            }
            b'\n' => {
                self.pos += 1;
                Some(SyntaxKind::Newline)
            }
            b'\r' => {
                self.pos += if self.peek(1) == b'\n' { 2 } else { 1 };
                Some(SyntaxKind::Newline)
            }
            b'/' => match self.peek(1) {
                b'/' => Some(self.line_comment()),
                b'*' => Some(self.block_comment()),
                _ => None,
            },
            0xEF if self.pos == 0 && self.bytes.starts_with("\u{feff}".as_bytes()) => {
                self.pos += 3;
                self.eat_while(|b| b == b' ' || b == b'\t');
                Some(SyntaxKind::Whitespace)
            }
            _ => None,
        }
    }

    fn line_comment(&mut self) -> SyntaxKind {
        let doc = self.peek(2) == b'/' && self.peek(3) != b'/';
        self.pos += 2;
        self.eat_while(|b| b != b'\n' && b != b'\r');
        if doc {
            SyntaxKind::DocComment
        } else {
            SyntaxKind::LineComment
        }
    }

    fn block_comment(&mut self) -> SyntaxKind {
        let start = self.pos;
        self.pos += 2;
        let len = self.bytes.len();
        let mut depth: usize = 1;
        while self.pos < len {
            match self.bytes[self.pos] {
                b'*' if self.peek(1) == b'/' => {
                    self.pos += 2;
                    depth -= 1;
                    if depth == 0 {
                        return SyntaxKind::BlockComment;
                    }
                }
                b'/' if self.peek(1) == b'*' => {
                    self.pos += 2;
                    depth += 1;
                }
                _ => self.pos += 1,
            }
        }
        let span = self.span(start, start + 2);
        self.diagnostic(
            Diagnostic::new(
                codes::UNTERMINATED_BLOCK_COMMENT,
                span,
                "unterminated block comment",
            )
            .with_help("close the comment with `*/`; block comments nest"),
        );
        SyntaxKind::BlockComment
    }

    /// Returns `true` if the next significant character after the current
    /// position (skipping whitespace, newlines and comments) is `{`.
    fn brace_follows(&self) -> bool {
        let bytes = self.bytes;
        let len = bytes.len();
        let mut i = self.pos;
        while i < len {
            match bytes[i] {
                b' ' | b'\t' | b'\n' | b'\r' => i += 1,
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    while i < len && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    i += 2;
                    let mut depth = 1usize;
                    while i < len && depth > 0 {
                        if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                            depth -= 1;
                            i += 2;
                        } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                            depth += 1;
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'{' => return true,
                _ => return false,
            }
        }
        false
    }

    // ---------------------------------------------------------- code mode

    fn code_token(&mut self) -> SyntaxKind {
        if let Some(kind) = self.trivia() {
            return kind;
        }
        let b = self.bytes[self.pos];
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.identifier(),
            b'0'..=b'9' => self.number(),
            b'"' => self.string_start(),
            b'#' => self.color(),
            b'{' => {
                self.pos += 1;
                if self.pending_style {
                    self.frames.push(Frame::Style {
                        depth: 0,
                        state: StyleState::Start,
                    });
                } else if let Some(Frame::Interp { depth, .. }) = self.frames.last_mut() {
                    *depth += 1;
                }
                SyntaxKind::LBrace
            }
            b'}' => {
                self.pos += 1;
                if let Some(Frame::Interp {
                    depth,
                    ends_statement,
                }) = self.frames.last_mut()
                {
                    if *depth == 0 {
                        let ends_statement = *ends_statement;
                        self.frames.pop();
                        if ends_statement {
                            self.set_style_state(StyleState::Start);
                        }
                        return SyntaxKind::InterpEnd;
                    }
                    *depth -= 1;
                }
                SyntaxKind::RBrace
            }
            _ => match self.punctuation(b) {
                Some(kind) => kind,
                None => self.error_run(false),
            },
        }
    }

    /// Lexes code-mode punctuation with maximal munch (grammar.md §3.7).
    fn punctuation(&mut self, b: u8) -> Option<SyntaxKind> {
        use SyntaxKind as K;
        let b1 = self.peek(1);
        let (kind, len) = match b {
            b'(' => (K::LParen, 1),
            b')' => (K::RParen, 1),
            b'[' => (K::LBrack, 1),
            b']' => (K::RBrack, 1),
            b',' => (K::Comma, 1),
            b':' => (K::Colon, 1),
            b';' => (K::Semicolon, 1),
            b'@' => (K::At, 1),
            b'~' => (K::Tilde, 1),
            b'.' => match (b1, self.peek(2)) {
                (b'.', b'<') => (K::DotDotLt, 3),
                (b'.', b'.') => (K::DotDotDot, 3),
                (b'.', _) => (K::DotDot, 2),
                _ => (K::Dot, 1),
            },
            b'?' => match b1 {
                b'.' => (K::QuestionDot, 2),
                b'?' => (K::QuestionQuestion, 2),
                _ => (K::Question, 1),
            },
            b'-' => match b1 {
                b'>' => (K::Arrow, 2),
                b'=' => (K::MinusEq, 2),
                _ => (K::Minus, 1),
            },
            b'=' => match b1 {
                b'>' => (K::FatArrow, 2),
                b'=' => (K::EqEq, 2),
                _ => (K::Eq, 1),
            },
            b'!' => match b1 {
                b'=' => (K::BangEq, 2),
                _ => (K::Bang, 1),
            },
            b'<' => match b1 {
                b'=' => (K::LtEq, 2),
                _ => (K::Lt, 1),
            },
            b'>' => match b1 {
                b'=' => (K::GtEq, 2),
                _ => (K::Gt, 1),
            },
            b'+' => match b1 {
                b'=' => (K::PlusEq, 2),
                _ => (K::Plus, 1),
            },
            b'*' => match b1 {
                b'=' => (K::StarEq, 2),
                _ => (K::Star, 1),
            },
            b'/' => match b1 {
                b'=' => (K::SlashEq, 2),
                _ => (K::Slash, 1),
            },
            b'%' => match b1 {
                b'=' => (K::PercentEq, 2),
                _ => (K::Percent, 1),
            },
            b'&' => match b1 {
                b'&' => (K::AmpAmp, 2),
                _ => (K::Amp, 1),
            },
            b'|' => match b1 {
                b'|' => (K::PipePipe, 2),
                _ => (K::Pipe, 1),
            },
            _ => return None,
        };
        self.pos += len;
        Some(kind)
    }

    fn identifier(&mut self) -> SyntaxKind {
        let start = self.pos;
        self.pos += 1;
        self.eat_while(is_ident_continue);
        let word = &self.bytes[start..self.pos];
        if word == b"_" {
            return SyntaxKind::Underscore;
        }
        match keyword(word) {
            Some(SyntaxKind::StyleKw) => {
                if !matches!(
                    self.last_significant,
                    SyntaxKind::Dot | SyntaxKind::QuestionDot
                ) && self.brace_follows()
                {
                    self.pending_style = true;
                }
                SyntaxKind::StyleKw
            }
            Some(kind) => kind,
            None => SyntaxKind::Ident,
        }
    }

    /// Lexes a number literal starting at a digit (or, in style mode, at a
    /// `-` that is followed by a digit or by `.` and a digit).
    fn number(&mut self) -> SyntaxKind {
        let start = self.pos;
        if self.bytes[self.pos] == b'-' {
            self.pos += 1;
        }
        if self.bytes.get(self.pos) == Some(&b'0') && matches!(self.peek(1), b'x' | b'b' | b'o') {
            let base = self.peek(1);
            self.pos += 2;
            let digits = self.pos;
            self.eat_while(is_ident_continue);
            let body = &self.bytes[digits..self.pos];
            let valid_digit = |c: u8| match base {
                b'x' => c.is_ascii_hexdigit(),
                b'b' => matches!(c, b'0' | b'1'),
                _ => matches!(c, b'0'..=b'7'),
            };
            let valid = body.iter().any(|&c| c != b'_')
                && body.iter().all(|&c| c == b'_' || valid_digit(c))
                && body.last() != Some(&b'_');
            if !valid {
                self.invalid_number(start);
            }
            return SyntaxKind::IntLit;
        }
        let mut kind = SyntaxKind::IntLit;
        let is_digit_or_sep = |b: u8| b.is_ascii_digit() || b == b'_';
        self.eat_while(is_digit_or_sep);
        if self.peek(0) == b'.' && self.peek(1).is_ascii_digit() {
            self.pos += 1;
            self.eat_while(is_digit_or_sep);
            kind = SyntaxKind::FloatLit;
        }
        if matches!(self.peek(0), b'e' | b'E')
            && (self.peek(1).is_ascii_digit()
                || (matches!(self.peek(1), b'+' | b'-') && self.peek(2).is_ascii_digit()))
        {
            self.pos += 2;
            self.eat_while(is_digit_or_sep);
            kind = SyntaxKind::FloatLit;
        }
        let numeric_end = self.pos;
        if self.peek(0) == b'%' {
            self.pos += 1;
            kind = SyntaxKind::UnitLit;
        } else if is_ident_start(self.peek(0)) {
            self.eat_while(is_ident_continue);
            kind = SyntaxKind::UnitLit;
        }
        if self.bytes.get(numeric_end.wrapping_sub(1)) == Some(&b'_') {
            self.invalid_number(start);
        }
        kind
    }

    fn invalid_number(&mut self, start: usize) {
        let span = self.span(start, self.pos);
        self.diagnostic(
            Diagnostic::new(codes::INVALID_NUMBER_LITERAL, span, "invalid number literal")
                .with_help(
                    "write digits after `0x`, `0b` or `0o` that are valid for the base, and do not end a number with `_`",
                ),
        );
    }

    fn color(&mut self) -> SyntaxKind {
        let start = self.pos;
        self.pos += 1;
        self.eat_while(is_ident_continue);
        let body = &self.bytes[start + 1..self.pos];
        let valid = matches!(body.len(), 3 | 4 | 6 | 8) && body.iter().all(u8::is_ascii_hexdigit);
        if !valid {
            let mut message = String::from("invalid color literal `");
            display_text(self.text.get(start..self.pos).unwrap_or("#"), &mut message);
            message.push('`');
            let span = self.span(start, self.pos);
            self.diagnostic(
                Diagnostic::new(codes::INVALID_COLOR_LITERAL, span, message)
                    .with_help("a color is `#` followed by 3, 4, 6 or 8 hexadecimal digits"),
            );
        }
        SyntaxKind::ColorLit
    }

    // -------------------------------------------------------- string mode

    fn string_start(&mut self) -> SyntaxKind {
        if self.open_strings == 0 {
            self.lowest_string = self.frames.len();
        }
        self.frames.push(Frame::String {
            open: off(self.pos),
        });
        self.open_strings += 1;
        self.pos += 1;
        SyntaxKind::StringStart
    }

    /// Lexes one piece of a string. Returns `None` (consuming nothing) when
    /// a line terminator ends the string.
    fn string_token(&mut self) -> Option<SyntaxKind> {
        let b = self.bytes[self.pos];
        match b {
            b'"' => {
                self.pos += 1;
                self.frames.pop();
                self.open_strings -= 1;
                Some(SyntaxKind::StringEnd)
            }
            b'\n' | b'\r' => {
                self.end_line_in_string();
                None
            }
            b'\\' => Some(self.escape()),
            b'{' => {
                if self.peek(1) == b'{' {
                    self.pos += 2;
                    Some(SyntaxKind::StringEscape)
                } else {
                    self.pos += 1;
                    self.frames.push(Frame::Interp {
                        depth: 0,
                        ends_statement: false,
                    });
                    Some(SyntaxKind::InterpStart)
                }
            }
            _ => {
                self.eat_while(|b| !matches!(b, b'"' | b'\\' | b'{' | b'\n' | b'\r'));
                Some(SyntaxKind::StringText)
            }
        }
    }

    fn escape(&mut self) -> SyntaxKind {
        let start = self.pos;
        let next = self.peek(1);
        match next {
            b'n' | b'r' | b't' | b'0' | b'\\' | b'"' => {
                self.pos += 2;
            }
            b'{' => {
                self.pos += 2;
                let span = self.span(start, self.pos);
                self.diagnostic(
                    Diagnostic::new(
                        codes::BRACE_ESCAPE,
                        span,
                        "`\\{` is not an escape; use `{{`",
                    )
                    .with_fix(Fix::replace(
                        "Replace `\\{` with `{{`",
                        span,
                        "{{",
                    )),
                );
            }
            b'u' => self.unicode_escape(start),
            b'\n' | b'\r' | 0 if self.pos + 1 >= self.bytes.len() || next != 0 => {
                // `\` at the end of the line or file.
                self.pos += 1;
                let span = self.span(start, self.pos);
                self.diagnostic(Diagnostic::new(
                    codes::INVALID_ESCAPE,
                    span,
                    "unknown escape `\\` at the end of the line",
                ));
            }
            _ => {
                let len = utf8_len(next).min(self.bytes.len() - self.pos - 1);
                self.pos += 1 + len;
                let mut message = String::from("unknown escape `");
                display_text(self.text.get(start..self.pos).unwrap_or("\\"), &mut message);
                message.push('`');
                let span = self.span(start, self.pos);
                self.diagnostic(
                    Diagnostic::new(codes::INVALID_ESCAPE, span, message).with_help(
                        "the escapes are `\\n`, `\\r`, `\\t`, `\\0`, `\\\\`, `\\\"` and `\\u{...}`",
                    ),
                );
            }
        }
        SyntaxKind::StringEscape
    }

    fn unicode_escape(&mut self, start: usize) {
        // At `\u`. The escape extends through the next `}` on this line
        // before any `"`, if `{` follows; otherwise it is just `\u`.
        let mut valid = false;
        let mut end = start + 2;
        if self.peek(2) == b'{' && start + 3 > self.unicode_scan_fail_until {
            let bytes = self.bytes;
            let mut i = start + 3;
            while i < bytes.len() && !matches!(bytes[i], b'}' | b'"' | b'\n' | b'\r') {
                i += 1;
            }
            if bytes.get(i) == Some(&b'}') {
                end = i + 1;
                let hex = &bytes[start + 3..i];
                valid = (1..=6).contains(&hex.len())
                    && hex.iter().all(u8::is_ascii_hexdigit)
                    && std::str::from_utf8(hex)
                        .ok()
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .and_then(char::from_u32)
                        .is_some();
            } else {
                self.unicode_scan_fail_until = i;
            }
        }
        self.pos = end;
        if !valid {
            let span = self.span(start, end);
            self.diagnostic(
                Diagnostic::new(
                    codes::INVALID_UNICODE_ESCAPE,
                    span,
                    "invalid unicode escape",
                )
                .with_help(
                    "write `\\u{...}` with 1 to 6 hexadecimal digits naming a Unicode scalar value",
                ),
            );
        }
    }

    // --------------------------------------------------------- style mode

    fn style_token(&mut self) -> SyntaxKind {
        if let Some(kind) = self.trivia() {
            return kind;
        }
        let b = self.bytes[self.pos];
        let Some(Frame::Style { depth, state }) = self.frames.last().copied() else {
            return self.error_run(true);
        };
        let state = if state == StyleState::Start {
            let next = if is_ident_start(b) {
                StyleState::Property
            } else {
                StyleState::Selector
            };
            self.set_style_state(next);
            next
        } else {
            state
        };
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                self.pos += 1;
                self.eat_while(|b| is_ident_continue(b) || b == b'-');
                SyntaxKind::Ident
            }
            b'0'..=b'9' => self.number(),
            b'-' if self.peek(1).is_ascii_digit()
                || (self.peek(1) == b'.' && self.peek(2).is_ascii_digit()) =>
            {
                if self.peek(1) == b'.' {
                    // `-.5`: a negative number with no integer part.
                    let start = self.pos;
                    self.pos += 2;
                    self.eat_while(|b| b.is_ascii_digit() || b == b'_');
                    let numeric_end = self.pos;
                    let mut kind = SyntaxKind::FloatLit;
                    if self.peek(0) == b'%' {
                        self.pos += 1;
                        kind = SyntaxKind::UnitLit;
                    } else if is_ident_start(self.peek(0)) {
                        self.eat_while(is_ident_continue);
                        kind = SyntaxKind::UnitLit;
                    }
                    if self.bytes[numeric_end - 1] == b'_' {
                        self.invalid_number(start);
                    }
                    kind
                } else {
                    self.number()
                }
            }
            b'"' => self.string_start(),
            b'#' => self.color(),
            b'{' => {
                self.pos += 1;
                if matches!(state, StyleState::Property | StyleState::Declaration) {
                    self.frames.push(Frame::Interp {
                        depth: 0,
                        ends_statement: state == StyleState::Property,
                    });
                    SyntaxKind::InterpStart
                } else {
                    self.set_style(depth + 1, StyleState::Start);
                    SyntaxKind::LBrace
                }
            }
            b'}' => {
                self.pos += 1;
                if depth == 0 {
                    self.frames.pop();
                } else {
                    self.set_style(depth - 1, StyleState::Start);
                }
                SyntaxKind::RBrace
            }
            b';' => {
                self.pos += 1;
                self.set_style_state(StyleState::Start);
                SyntaxKind::Semicolon
            }
            _ => {
                use SyntaxKind as K;
                let kind = match b {
                    b'(' => K::LParen,
                    b')' => K::RParen,
                    b'[' => K::LBrack,
                    b']' => K::RBrack,
                    b',' => K::Comma,
                    b':' => {
                        if state == StyleState::Property {
                            self.set_style_state(StyleState::Declaration);
                        }
                        K::Colon
                    }
                    b'.' => K::Dot,
                    b'&' => K::Amp,
                    b'>' => K::Gt,
                    b'+' => K::Plus,
                    b'~' => K::Tilde,
                    b'*' => K::Star,
                    b'/' => K::Slash,
                    b'!' => K::Bang,
                    b'=' => K::Eq,
                    b'-' => K::Minus,
                    b'%' => K::Percent,
                    b'@' => K::At,
                    _ => return self.error_run(true),
                };
                self.pos += 1;
                kind
            }
        }
    }

    fn set_style_state(&mut self, new_state: StyleState) {
        if let Some(Frame::Style { state, .. }) = self.frames.last_mut() {
            *state = new_state;
        }
    }

    fn set_style(&mut self, new_depth: u32, new_state: StyleState) {
        if let Some(Frame::Style { depth, state }) = self.frames.last_mut() {
            *depth = new_depth;
            *state = new_state;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<SyntaxKind> {
        lex(FileId::new(0), text)
            .kinds()
            .iter()
            .copied()
            .filter(|k| *k != SyntaxKind::Whitespace)
            .collect()
    }

    #[test]
    fn empty_input_has_no_tokens() {
        let lexed = lex(FileId::new(0), "");
        assert!(lexed.is_empty());
        assert_eq!(lexed.range(0), None);
        assert_eq!(lexed.text(0), None);
        assert_eq!(lexed.starts(), [0]);
    }

    #[test]
    fn builder_tracks_offsets_and_fills_the_rest() {
        let text = "let x";
        let mut builder = LexedBuilder::new(text);
        builder.push(SyntaxKind::LetKw, 3);
        builder.push(SyntaxKind::Whitespace, 0);
        builder.push(SyntaxKind::Whitespace, 1);
        let lexed = builder.finish();
        let tokens: Vec<_> = lexed.iter().collect();
        assert_eq!(
            tokens,
            vec![
                (SyntaxKind::LetKw, "let"),
                (SyntaxKind::Whitespace, " "),
                (SyntaxKind::Error, "x")
            ]
        );
        assert_eq!(lexed.range(2), Some(TextRange::new(4.into(), 5.into())));
        assert_eq!(lexed.kind(3), None);
    }

    #[test]
    fn keyword_table_matches_the_kind_table() {
        for kind in SyntaxKind::ALL.iter().copied().filter(|k| k.is_keyword()) {
            let text = kind.fixed_text().unwrap_or_default();
            assert_eq!(keyword(text.as_bytes()), Some(kind), "{text}");
        }
        assert_eq!(keyword(b"component"), None);
        assert_eq!(keyword(b""), None);
        assert_eq!(keyword(b"Let"), None);
    }

    #[test]
    fn style_mode_is_entered_only_before_a_brace() {
        use SyntaxKind as K;
        assert_eq!(
            kinds("style { a-b }"),
            [K::StyleKw, K::LBrace, K::Ident, K::RBrace]
        );
        assert_eq!(
            kinds("style ( a-b )"),
            [
                K::StyleKw,
                K::LParen,
                K::Ident,
                K::Minus,
                K::Ident,
                K::RParen
            ]
        );
        assert_eq!(
            kinds("x.style { a-b }"),
            [
                K::Ident,
                K::Dot,
                K::StyleKw,
                K::LBrace,
                K::Ident,
                K::Minus,
                K::Ident,
                K::RBrace
            ]
        );
    }

    #[test]
    fn style_declarations_and_selectors_treat_braces_differently() {
        use SyntaxKind as K;
        assert_eq!(
            kinds("style { a: {x}; .b { } }"),
            [
                K::StyleKw,
                K::LBrace,
                K::Ident,
                K::Colon,
                K::InterpStart,
                K::Ident,
                K::InterpEnd,
                K::Semicolon,
                K::Dot,
                K::Ident,
                K::LBrace,
                K::RBrace,
                K::RBrace
            ]
        );
    }

    #[test]
    fn negative_leading_dot_numbers_in_style_mode() {
        use SyntaxKind as K;
        assert_eq!(
            kinds("style { .a { b: -.5 -.5px; } }")[7..10],
            [K::FloatLit, K::UnitLit, K::Semicolon]
        );
    }

    #[test]
    fn unicode_escape_scan_without_closing_brace_stays_linear() {
        let text = format!("\"{}", "\\u{".repeat(50_000));
        let lexed = lex(FileId::new(0), &text);
        // One K0008 per `\u` and one K0002 for the string.
        assert_eq!(lexed.diagnostics().len(), 50_001);
    }

    #[test]
    fn display_text_escapes_controls_and_truncates() {
        let mut out = String::new();
        display_text("a\u{1}\u{feff}", &mut out);
        assert_eq!(out, "a\\u{1}\\u{feff}");
        let mut long = String::new();
        display_text(&"$".repeat(100), &mut long);
        assert_eq!(long.chars().count(), 25);
    }

    #[test]
    fn utf8_lengths() {
        for c in ['a', 'é', '日', '😀'] {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            assert_eq!(utf8_len(s.as_bytes()[0]), c.len_utf8());
        }
    }
}
