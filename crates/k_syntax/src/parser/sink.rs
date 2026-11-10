//! The tree sink: turns parser events into a rowan green tree.
//!
//! The parser works on the trivia-free view of the tokens; the sink walks
//! the full lexer token stream alongside the events and re-inserts every
//! trivia token, so the tree is lossless. Trivia placement follows
//! `docs/compiler/architecture.md` ("Parser"):
//!
//! * trivia before a node's first token is emitted *before* the node starts,
//!   so a node's text range runs exactly from its first to its last
//!   significant token, and trailing comments stay outside the following
//!   node;
//! * doc comments (`///`) directly before an item or member are emitted
//!   *inside* that node, as its first children, so the declaration "owns"
//!   its documentation (grammar.md RA-29);
//! * trivia after the last token goes into the root `SourceFile`.
//!
//! # Building
//!
//! The sink builds green nodes bottom-up with [`GreenNode::new`] instead of
//! rowan's `GreenNodeBuilder`. The builder's node cache rehashes cached
//! nodes with a hash that recurses over their whole subtree whenever its
//! table grows; for a long chain of small nodes (`a.b.c.d...`) that costs
//! quadratic time and unbounded stack. Here, tokens are interned per parse
//! (keyed by slices of the source, so lookups never allocate) and nodes are
//! not deduplicated.
//!
//! The sink never fails: unexpected event sequences (which would be parser
//! bugs) degrade gracefully, and tokens the parser did not consume are
//! appended to the root inside an `ErrorNode`, so the tree always covers the
//! whole input.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use rowan::{GreenNode, GreenToken, NodeOrToken};

use super::engine::{Event, is_tombstone};
use crate::kind::SyntaxKind;
use crate::lexer::Lexed;

/// A child of a green node under construction.
type GreenElement = NodeOrToken<GreenNode, GreenToken>;

/// Tokens longer than this are not interned (they rarely repeat).
const INTERN_MAX_LEN: usize = 24;

/// Runs of up to this many spaces are cached by length (see [`TokenCache`]).
const SPACES_MAX_LEN: usize = 32;

/// The number of [`SyntaxKind`]s, the length of the per-kind cache.
const KIND_COUNT: usize = SyntaxKind::ALL.len();

/// `HAS_FIXED_TEXT[kind as usize]` is `kind.fixed_text().is_some()`,
/// precomputed so the hot path is one load.
const HAS_FIXED_TEXT: [bool; KIND_COUNT] = {
    let mut table = [false; KIND_COUNT];
    let mut index = 0;
    while index < KIND_COUNT {
        table[index] = SyntaxKind::ALL[index].fixed_text().is_some();
        index += 1;
    }
    table
};

/// A small, fast, non-cryptographic hasher (the Fx hash of rustc) for the
/// token interner. Keys are trusted source slices of bounded length.
#[derive(Default)]
struct FxHasher(u64);

impl Hasher for FxHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Eight bytes at a time, then the tail: the same mixing step as the
        // byte-wise loop, a fraction of the work for identifiers.
        let (chunks, rest) = bytes.as_chunks::<8>();
        for chunk in chunks {
            self.write_u64(u64::from_le_bytes(*chunk));
        }
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            // Fold the tail length in, so "a" and "a\0" hash differently.
            self.write_u64(u64::from_le_bytes(word) ^ ((rest.len() as u64) << 59));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.write_u64(u64::from(value));
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

/// Returns `true` for node kinds that own the doc comments before them.
fn takes_doc_comments(kind: SyntaxKind) -> bool {
    use SyntaxKind as K;
    matches!(
        kind,
        K::ComponentDecl
            | K::PageDecl
            | K::LayoutDecl
            | K::ServiceDecl
            | K::ProtocolDecl
            | K::EntityDecl
            | K::RepositoryDecl
            | K::StructDecl
            | K::EnumDecl
            | K::ClassDecl
            | K::InterfaceDecl
            | K::ThemeDecl
            | K::TestSuiteDecl
            | K::TestCaseDecl
            | K::AppDecl
            | K::ConfigDecl
            | K::MigrationDecl
            | K::ExtendDecl
            | K::FuncDecl
            | K::FieldDecl
            | K::InitDecl
            | K::RpcDecl
            | K::EnumCase
            | K::UseDecl
            | K::ThemeToken
            | K::ThemeVariant
            | K::ThemeOverride
            | K::ViewBlock
            | K::StyleBlock
            | K::LoadingBlock
            | K::FailedBlock
            | K::MetadataBlock
    )
}

/// The token interner of one parse.
///
/// Most tokens need no hashing at all: keywords and punctuation have one
/// text per kind and are cached in a slot per kind, and the most common
/// trivia (a newline, a run of spaces) is cached by length. Everything else
/// up to [`INTERN_MAX_LEN`] bytes goes through a hash map keyed by the kind
/// and a slice of the source, so lookups never allocate. On the realistic
/// benchmark input the slots serve about three quarters of all tokens; see
/// `docs/compiler/performance.md`.
struct TokenCache<'a> {
    /// One token per kind with fixed text, indexed by `kind as usize`.
    fixed: Vec<Option<GreenToken>>,
    /// `spaces[n]` is a `Whitespace` token of `n` spaces.
    spaces: [Option<GreenToken>; SPACES_MAX_LEN + 1],
    /// The `Newline` token `"\n"`.
    newline: Option<GreenToken>,
    /// Every other short token.
    map: HashMap<(SyntaxKind, &'a str), GreenToken, BuildHasherDefault<FxHasher>>,
}

impl<'a> TokenCache<'a> {
    /// Creates an empty cache with a slot for every kind.
    fn new() -> TokenCache<'a> {
        TokenCache {
            fixed: vec![None; KIND_COUNT],
            spaces: std::array::from_fn(|_| None),
            newline: None,
            map: HashMap::with_capacity_and_hasher(256, BuildHasherDefault::default()),
        }
    }

    /// Returns the green token for `kind` and `text`, creating it on first
    /// use.
    ///
    /// `lexed_kind` is the kind the lexer gave the token. When it equals
    /// `kind` and the kind has fixed text, the text is known to be that
    /// fixed text (a lexer invariant checked by the property tests and the
    /// fuzzers), so the per-kind slot is used without comparing texts. A
    /// remapped token (a contextual keyword) is compared first.
    fn get(&mut self, kind: SyntaxKind, lexed_kind: SyntaxKind, text: &'a str) -> GreenToken {
        let fixed = HAS_FIXED_TEXT.get(kind as usize).copied().unwrap_or(false);
        let slot = match kind {
            _ if fixed && (kind == lexed_kind || kind.fixed_text() == Some(text)) => {
                debug_assert_eq!(kind.fixed_text(), Some(text), "lexer invariant");
                self.fixed.get_mut(kind as usize)
            }
            SyntaxKind::Newline if text == "\n" => Some(&mut self.newline),
            SyntaxKind::Whitespace
                if text.len() <= SPACES_MAX_LEN && text.bytes().all(|b| b == b' ') =>
            {
                self.spaces.get_mut(text.len())
            }
            _ => None,
        };
        if let Some(slot) = slot {
            return slot
                .get_or_insert_with(|| GreenToken::new(kind.into(), text))
                .clone();
        }
        if text.len() <= INTERN_MAX_LEN {
            self.map
                .entry((kind, text))
                .or_insert_with(|| GreenToken::new(kind.into(), text))
                .clone()
        } else {
            GreenToken::new(kind.into(), text)
        }
    }
}

/// The state of one tree build: a stack of open nodes over a shared child
/// buffer, and a cursor into the full lexer token stream.
///
/// Invariant: every lexer token before `raw` has been pushed into
/// `children` exactly once, in order, which is what makes the tree
/// lossless.
struct Sink<'a> {
    /// The full token stream, trivia included.
    lexed: &'a Lexed<'a>,
    /// Open nodes: kind and the index in `children` where its children start.
    open: Vec<(SyntaxKind, usize)>,
    /// Children of all open nodes, innermost last.
    children: Vec<GreenElement>,
    /// The token interner.
    tokens: TokenCache<'a>,
    /// Index of the next lexer token to emit.
    raw: usize,
    /// The finished root.
    root: Option<GreenNode>,
    /// The deepest nesting reached.
    max_depth: usize,
}

impl<'a> Sink<'a> {
    /// Creates a sink at the start of `lexed` with no open nodes.
    fn new(lexed: &'a Lexed<'a>) -> Sink<'a> {
        Sink {
            lexed,
            open: Vec::with_capacity(64),
            children: Vec::with_capacity(256),
            tokens: TokenCache::new(),
            raw: 0,
            root: None,
            max_depth: 0,
        }
    }

    /// Returns the number of open nodes.
    fn depth(&self) -> usize {
        self.open.len()
    }

    /// Appends lexer token `index` as `kind` to the innermost open node.
    fn push_token(&mut self, index: usize, kind: SyntaxKind) {
        let text = self.lexed.text(index).unwrap_or("");
        let lexed_kind = self.lexed.kinds().get(index).copied().unwrap_or(kind);
        let token = self.tokens.get(kind, lexed_kind, text);
        self.children.push(NodeOrToken::Token(token));
    }

    /// Index of the next non-trivia lexer token at or after `self.raw`.
    fn next_significant(&self) -> usize {
        let kinds = self.lexed.kinds();
        let mut index = self.raw;
        while index < kinds.len() && kinds[index].is_trivia() {
            index += 1;
        }
        index
    }

    /// Emits trivia tokens up to (not including) lexer token `until`.
    fn emit_trivia_until(&mut self, until: usize) {
        let kinds = self.lexed.kinds();
        while self.raw < until && self.raw < kinds.len() {
            self.push_token(self.raw, kinds[self.raw]);
            self.raw += 1;
        }
    }

    /// Where the trivia before a doc-comment-owning node splits: the index
    /// of the first doc comment of the run of doc comments, whitespace and
    /// newlines directly before the node, or the next token if there is none.
    fn doc_split(&self) -> usize {
        let kinds = self.lexed.kinds();
        let next = self.next_significant();
        let mut split = next;
        let mut index = next;
        while index > self.raw {
            match kinds[index - 1] {
                SyntaxKind::DocComment => {
                    index -= 1;
                    split = index;
                }
                SyntaxKind::Whitespace | SyntaxKind::Newline => index -= 1,
                _ => break,
            }
        }
        split
    }

    /// Opens a node of `kind`.
    ///
    /// The trivia before the node's first token is emitted into the parent
    /// first, except for the doc comments that a declaration-like `kind`
    /// owns ([`takes_doc_comments`]). Ignored once the root is finished.
    fn start_node(&mut self, kind: SyntaxKind) {
        if self.root.is_some() {
            return;
        }
        if self.depth() > 0 {
            let until = if takes_doc_comments(kind) {
                self.doc_split()
            } else {
                self.next_significant()
            };
            self.emit_trivia_until(until);
        }
        self.open.push((kind, self.children.len()));
        self.max_depth = self.max_depth.max(self.depth());
    }

    /// Emits the pending trivia and then the next significant lexer token as
    /// `kind`. Ignored once the root is finished or past the last token.
    fn token(&mut self, kind: SyntaxKind) {
        if self.root.is_some() {
            return;
        }
        let next = self.next_significant();
        self.emit_trivia_until(next);
        if next < self.lexed.len() {
            self.push_token(next, kind);
            self.raw = next + 1;
        }
    }

    /// Closes the innermost open node.
    ///
    /// Closing the root first emits every remaining token into it (see
    /// [`Sink::emit_rest`]), so trailing trivia and unconsumed tokens stay in
    /// the tree. A `Finish` with no open node is ignored.
    fn finish_node(&mut self) {
        if self.depth() == 1 {
            self.emit_rest();
        }
        let Some((kind, first)) = self.open.pop() else {
            return;
        };
        let node = GreenNode::new(kind.into(), self.children.drain(first..));
        if self.open.is_empty() {
            self.root = Some(node);
        } else {
            self.children.push(NodeOrToken::Node(node));
        }
    }

    /// Emits every remaining lexer token into the current (root) node,
    /// wrapping runs of unconsumed significant tokens in `ErrorNode`s.
    fn emit_rest(&mut self) {
        let kinds = self.lexed.kinds();
        while self.raw < kinds.len() {
            let kind = kinds[self.raw];
            if kind.is_trivia() {
                self.push_token(self.raw, kind);
                self.raw += 1;
                continue;
            }
            let first = self.children.len();
            while self.raw < kinds.len() && !kinds[self.raw].is_trivia() {
                self.push_token(self.raw, kinds[self.raw]);
                self.raw += 1;
            }
            let node = GreenNode::new(SyntaxKind::ErrorNode.into(), self.children.drain(first..));
            self.children.push(NodeOrToken::Node(node));
        }
    }
}

/// Builds the green tree for `lexed` from the parser's `events`, and returns
/// it with its depth (the root alone has depth 1).
///
/// The events must describe a single root node; if they do not (a parser
/// bug), the result is still a single `SourceFile` root containing every
/// token.
pub(crate) fn build(lexed: &Lexed<'_>, mut events: Vec<Event>) -> (GreenNode, u32) {
    let mut sink = Sink::new(lexed);
    let mut parents: Vec<SyntaxKind> = Vec::new();
    let placeholder = || Event::Start {
        kind: SyntaxKind::Whitespace,
        forward_parent: None,
    };
    for index in 0..events.len() {
        if sink.root.is_some() {
            break;
        }
        match std::mem::replace(&mut events[index], placeholder()) {
            Event::Start {
                kind,
                forward_parent,
            } => {
                if is_tombstone(kind) && forward_parent.is_none() {
                    continue;
                }
                parents.clear();
                if !is_tombstone(kind) {
                    parents.push(kind);
                }
                let mut cursor = index;
                let mut link = forward_parent;
                while let Some(distance) = link {
                    cursor += distance.get() as usize;
                    let Some(slot) = events.get_mut(cursor) else {
                        break;
                    };
                    match std::mem::replace(slot, placeholder()) {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            if !is_tombstone(kind) {
                                parents.push(kind);
                            }
                            link = forward_parent;
                        }
                        other => {
                            // Not a start event: restore it and stop.
                            *slot = other;
                            break;
                        }
                    }
                }
                for kind in parents.iter().rev() {
                    sink.start_node(*kind);
                }
            }
            Event::Token { kind } => {
                if sink.depth() == 0 {
                    sink.start_node(SyntaxKind::SourceFile);
                }
                sink.token(kind);
            }
            Event::Finish => sink.finish_node(),
        }
    }
    if sink.root.is_none() && sink.depth() == 0 {
        sink.start_node(SyntaxKind::SourceFile);
    }
    while sink.root.is_none() && sink.depth() > 0 {
        sink.finish_node();
    }
    let depth = u32::try_from(sink.max_depth).unwrap_or(u32::MAX);
    let root = sink
        .root
        .unwrap_or_else(|| GreenNode::new(SyntaxKind::SourceFile.into(), std::iter::empty()));
    (root, depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    #[test]
    fn fx_hasher_distinguishes_kinds_and_texts() {
        let hash = |kind: SyntaxKind, text: &str| {
            let mut h = FxHasher::default();
            (kind, text).hash(&mut h);
            h.finish()
        };
        assert_ne!(hash(SyntaxKind::Ident, "a"), hash(SyntaxKind::Ident, "b"));
        assert_ne!(hash(SyntaxKind::Ident, "a"), hash(SyntaxKind::Error, "a"));
        assert_eq!(hash(SyntaxKind::Ident, "a"), hash(SyntaxKind::Ident, "a"));
    }

    #[test]
    fn fx_hasher_distinguishes_texts_across_chunk_boundaries() {
        let hash = |text: &str| {
            let mut h = FxHasher::default();
            h.write(text.as_bytes());
            h.finish()
        };
        let texts = [
            "",
            "a",
            "a\0",
            "\0a",
            "abcdefgh",
            "abcdefghi",
            "abcdefgi",
            "abcdefgh\0",
            "bacdefgh",
        ];
        for (i, a) in texts.iter().enumerate() {
            for b in &texts[i + 1..] {
                assert_ne!(hash(a), hash(b), "{a:?} and {b:?} collide");
            }
        }
    }

    /// Whether two green tokens are the same allocation.
    fn same(a: &GreenToken, b: &GreenToken) -> bool {
        std::ptr::eq::<rowan::GreenTokenData>(&**a, &**b)
    }

    #[test]
    fn token_cache_shares_tokens_with_equal_kind_and_text() {
        let mut cache = TokenCache::new();
        let source = String::from("let let    \n\n\t\t name name {");
        let cases = [
            (SyntaxKind::LetKw, &source[0..3], &source[4..7]),
            (SyntaxKind::Whitespace, &source[7..11], &source[7..11]),
            (SyntaxKind::Newline, &source[11..12], &source[12..13]),
            (SyntaxKind::Whitespace, &source[13..15], &source[13..15]),
            (SyntaxKind::Ident, &source[16..20], &source[21..25]),
        ];
        for (kind, a, b) in cases {
            let first = cache.get(kind, kind, a);
            let second = cache.get(kind, kind, b);
            assert!(same(&first, &second), "{kind:?} {a:?} is not shared");
            assert_eq!(first.text(), a);
            assert_eq!(SyntaxKind::from_raw(first.kind().0), kind);
        }
    }

    #[test]
    fn token_cache_never_confuses_kinds_or_texts() {
        let mut cache = TokenCache::new();
        // Same text, different kinds (a remapped contextual keyword).
        let ident = cache.get(SyntaxKind::Ident, SyntaxKind::Ident, "page");
        let keyword = cache.get(SyntaxKind::PageKw, SyntaxKind::Ident, "page");
        assert!(!same(&ident, &keyword));
        assert_eq!(SyntaxKind::from_raw(keyword.kind().0), SyntaxKind::PageKw);
        // A fixed-text kind with an unusual text bypasses the slot.
        let odd = cache.get(SyntaxKind::LBrace, SyntaxKind::Ident, "x");
        assert_eq!(odd.text(), "x");
        let brace = cache.get(SyntaxKind::LBrace, SyntaxKind::LBrace, "{");
        assert_eq!(brace.text(), "{");
        // Different runs of spaces are different tokens; long runs and tabs
        // still round-trip.
        let long = " ".repeat(SPACES_MAX_LEN + 1);
        for text in [" ", "  ", "\t", " \t", long.as_str(), "\r\n", "\r"] {
            let kind = if text.starts_with('\r') {
                SyntaxKind::Newline
            } else {
                SyntaxKind::Whitespace
            };
            assert_eq!(cache.get(kind, kind, text).text(), text);
        }
        assert!(!same(
            &cache.get(SyntaxKind::Whitespace, SyntaxKind::Whitespace, " "),
            &cache.get(SyntaxKind::Whitespace, SyntaxKind::Whitespace, "  ")
        ));
    }

    #[test]
    fn doc_comment_owners_are_items_and_members() {
        assert!(takes_doc_comments(SyntaxKind::FuncDecl));
        assert!(takes_doc_comments(SyntaxKind::ViewBlock));
        assert!(!takes_doc_comments(SyntaxKind::Import));
        assert!(!takes_doc_comments(SyntaxKind::Annotation));
        assert!(!takes_doc_comments(SyntaxKind::Block));
    }
}
