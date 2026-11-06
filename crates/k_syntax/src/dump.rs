//! Text dumps of token streams and syntax trees for golden tests.
//!
//! * [`dump_tokens`] writes the `.tokens` format: one token per line, as
//!   `Kind "text"`, with [`SyntaxKind::Whitespace`] tokens omitted.
//! * [`dump_tree`] writes the `.ast` format: the compact S-expression dump of
//!   a syntax tree, in canonical layout.
//!
//! Both formats are specified in `docs/compiler/grammar.md` ("Syntax tree
//! dump format") and `docs/compiler/testing.md`, which are normative; this
//! module implements them.

use std::fmt;

use rowan::WalkEvent;

use crate::kind::SyntaxKind;
use crate::lexer::Lexed;
use crate::sexpr::{self, Sexpr};
use crate::syntax::{SyntaxElement, SyntaxNode};

/// Writes the `.tokens` dump of a token stream.
///
/// ```
/// use k_syntax::{dump::dump_tokens, lexer::LexedBuilder, SyntaxKind};
///
/// let mut b = LexedBuilder::new("let x\n");
/// b.push(SyntaxKind::LetKw, 3);
/// b.push(SyntaxKind::Whitespace, 1);
/// b.push(SyntaxKind::Ident, 1);
/// b.push(SyntaxKind::Newline, 1);
/// assert_eq!(dump_tokens(&b.finish()), "LetKw \"let\"\nIdent \"x\"\nNewline \"\\n\"\n");
/// ```
#[must_use]
pub fn dump_tokens(lexed: &Lexed<'_>) -> String {
    let mut out = String::with_capacity(lexed.len() * 16);
    for (kind, text) in lexed.iter() {
        if kind == SyntaxKind::Whitespace {
            continue;
        }
        out.push_str(kind.name());
        out.push(' ');
        sexpr::write_quoted(&mut out, text);
        out.push('\n');
    }
    out
}

/// One entry of a `.tokens` file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TokenLine {
    /// The token kind.
    pub kind: SyntaxKind,
    /// The decoded token text.
    pub text: String,
}

impl fmt::Display for TokenLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        out.push_str(self.kind.name());
        out.push(' ');
        sexpr::write_quoted(&mut out, &self.text);
        f.write_str(&out)
    }
}

/// An error reading a `.tokens` file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TokensError {
    /// 1-based line number.
    pub line: usize,
    /// What is wrong with the line.
    pub message: String,
}

impl fmt::Display for TokensError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for TokensError {}

/// Reads a `.tokens` file.
///
/// Blank lines and lines whose first non-blank character is `#` are ignored.
/// Every other line is `Kind "text"`, where `Kind` is a [`SyntaxKind`]
/// variant name and `"text"` is quoted with the escapes of the S-expression
/// format. `Whitespace` lines are rejected, because the dump omits
/// whitespace.
///
/// # Errors
///
/// Returns the first malformed line.
pub fn parse_tokens_dump(text: &str) -> Result<Vec<TokenLine>, TokensError> {
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let error = |message: String| TokensError {
            line: number,
            message,
        };
        let (name, rest) = trimmed
            .split_once(char::is_whitespace)
            .ok_or_else(|| error("expected `Kind \"text\"`".into()))?;
        let kind = SyntaxKind::from_name(name)
            .filter(|kind| kind.is_token())
            .ok_or_else(|| error(format!("`{name}` is not a token kind")))?;
        if kind == SyntaxKind::Whitespace {
            return Err(error(
                "`Whitespace` tokens are omitted from `.tokens` files".into(),
            ));
        }
        let parsed = sexpr::parse(rest.trim()).map_err(|e| error(e.message))?;
        let [Sexpr::String(text)] = parsed.as_slice() else {
            return Err(error(
                "expected exactly one quoted string after the kind".into(),
            ));
        };
        lines.push(TokenLine {
            kind,
            text: text.clone(),
        });
    }
    Ok(lines)
}

/// Returns the `.tokens` entries of a token stream (whitespace omitted).
#[must_use]
pub fn token_lines(lexed: &Lexed<'_>) -> Vec<TokenLine> {
    lexed
        .iter()
        .filter(|(kind, _)| *kind != SyntaxKind::Whitespace)
        .map(|(kind, text)| TokenLine {
            kind,
            text: text.to_owned(),
        })
        .collect()
}

/// Builds the S-expression dump of a syntax tree.
///
/// The rules (normative in `docs/compiler/grammar.md`):
///
/// 1. A node becomes a list whose head is its kind name.
/// 2. Tokens for which [`SyntaxKind::is_dump_omitted`] holds are left out,
///    and so are the `<` and `>` of `GenericParamList` and `GenericArgList`.
/// 3. Consecutive [`SyntaxKind::StringText`] and [`SyntaxKind::StringEscape`]
///    tokens become one quoted atom holding the *decoded* string value.
/// 4. Every other token becomes an atom of its text, bare if possible and
///    quoted otherwise.
///
/// The tree is walked without recursion, so trees of any depth (a
/// 200,000-term sum nests 200,000 `BinaryExpr` nodes) can be dumped on a
/// small stack.
#[must_use]
pub fn tree_sexpr(node: &SyntaxNode) -> Sexpr {
    /// One open list: its items, the string atom being decoded, and whether
    /// it is a generic list whose `<` and `>` are omitted.
    struct Frame {
        items: Vec<Sexpr>,
        pending: Option<String>,
        angle_list: bool,
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut result = None;
    for event in node.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(SyntaxElement::Node(child)) => {
                let kind = child.kind();
                stack.push(Frame {
                    items: vec![Sexpr::Symbol(kind.name().to_owned())],
                    pending: None,
                    angle_list: matches!(
                        kind,
                        SyntaxKind::GenericParamList | SyntaxKind::GenericArgList
                    ),
                });
            }
            WalkEvent::Leave(SyntaxElement::Node(_)) => {
                let Some(mut frame) = stack.pop() else {
                    continue;
                };
                flush(&mut frame.pending, &mut frame.items);
                let list = Sexpr::List(frame.items);
                match stack.last_mut() {
                    Some(parent) => {
                        flush(&mut parent.pending, &mut parent.items);
                        parent.items.push(list);
                    }
                    None => result = Some(list),
                }
            }
            WalkEvent::Enter(SyntaxElement::Token(token)) => {
                let Some(frame) = stack.last_mut() else {
                    continue;
                };
                let kind = token.kind();
                if matches!(kind, SyntaxKind::StringText | SyntaxKind::StringEscape) {
                    let buffer = frame.pending.get_or_insert_with(String::new);
                    if kind == SyntaxKind::StringText {
                        buffer.push_str(token.text());
                    } else {
                        decode_escape(token.text(), buffer);
                    }
                } else if !kind.is_dump_omitted()
                    && !(frame.angle_list && matches!(kind, SyntaxKind::Lt | SyntaxKind::Gt))
                {
                    flush(&mut frame.pending, &mut frame.items);
                    frame.items.push(Sexpr::atom(token.text()));
                }
            }
            WalkEvent::Leave(SyntaxElement::Token(_)) => {}
        }
    }
    result.unwrap_or_else(|| Sexpr::List(vec![Sexpr::Symbol(node.kind().name().to_owned())]))
}

fn flush(pending: &mut Option<String>, items: &mut Vec<Sexpr>) {
    if let Some(text) = pending.take() {
        items.push(Sexpr::String(text));
    }
}

/// Writes the `.ast` dump of a syntax tree in canonical layout, ending with a
/// newline.
#[must_use]
pub fn dump_tree(node: &SyntaxNode) -> String {
    sexpr::pretty(&tree_sexpr(node))
}

/// Appends the decoded value of one [`SyntaxKind::StringEscape`] token.
///
/// Recognized escapes are `\n`, `\r`, `\t`, `\0`, `\\`, `\"`, `\u{HEX}` and
/// `{{`. Anything else (which the lexer has already reported) is appended
/// verbatim, so invalid input still produces a readable dump.
pub fn decode_escape(text: &str, out: &mut String) {
    let decoded = match text {
        "\\n" => Some('\n'),
        "\\r" => Some('\r'),
        "\\t" => Some('\t'),
        "\\0" => Some('\0'),
        "\\\\" => Some('\\'),
        "\\\"" => Some('"'),
        "{{" => Some('{'),
        _ => text
            .strip_prefix("\\u{")
            .and_then(|rest| rest.strip_suffix('}'))
            .filter(|hex| (1..=6).contains(&hex.len()))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .and_then(char::from_u32),
    };
    match decoded {
        Some(c) => out.push(c),
        None => out.push_str(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::LexedBuilder;
    use rowan::GreenNodeBuilder;

    fn node(build: impl FnOnce(&mut GreenNodeBuilder<'static>)) -> SyntaxNode {
        let mut b = GreenNodeBuilder::new();
        build(&mut b);
        SyntaxNode::new_root(b.finish())
    }

    fn tok(b: &mut GreenNodeBuilder<'static>, kind: SyntaxKind, text: &str) {
        b.token(kind.into(), text);
    }

    #[test]
    fn tokens_dump_omits_whitespace_and_escapes_text() {
        let text = "a \"\t\"\n";
        let mut b = LexedBuilder::new(text);
        for (kind, len) in [
            (SyntaxKind::Ident, 1),
            (SyntaxKind::Whitespace, 1),
            (SyntaxKind::StringStart, 1),
            (SyntaxKind::StringText, 1),
            (SyntaxKind::StringEnd, 1),
            (SyntaxKind::Newline, 1),
        ] {
            b.push(kind, len);
        }
        let lexed = b.finish();
        let dump = dump_tokens(&lexed);
        assert_eq!(
            dump,
            "Ident \"a\"\nStringStart \"\\\"\"\nStringText \"\\t\"\nStringEnd \"\\\"\"\nNewline \"\\n\"\n"
        );
        assert_eq!(parse_tokens_dump(&dump).unwrap(), token_lines(&lexed));
    }

    #[test]
    fn tokens_dump_reader_accepts_comments_and_rejects_garbage() {
        let ok = "# comment\n\n  Ident   \"x\"  \n";
        assert_eq!(
            parse_tokens_dump(ok).unwrap(),
            vec![TokenLine {
                kind: SyntaxKind::Ident,
                text: "x".into()
            }]
        );
        for (bad, message) in [
            ("Ident", "expected"),
            ("Bogus \"x\"", "not a token kind"),
            ("SourceFile \"x\"", "not a token kind"),
            ("Whitespace \" \"", "omitted"),
            ("Ident x", "quoted string"),
            ("Ident \"x\" \"y\"", "quoted string"),
            ("Ident \"x", "unclosed"),
        ] {
            let error = parse_tokens_dump(bad).unwrap_err();
            assert!(error.message.contains(message), "{bad}: {}", error.message);
            assert_eq!(error.line, 1);
        }
    }

    #[test]
    fn tree_dump_follows_the_omission_and_string_rules() {
        // text { "a\n{{b}" } with an interpolation inside the string
        let root = node(|b| {
            b.start_node(SyntaxKind::SourceFile.into());
            b.start_node(SyntaxKind::CallExpr.into());
            b.start_node(SyntaxKind::NameExpr.into());
            tok(b, SyntaxKind::Ident, "text");
            b.finish_node();
            tok(b, SyntaxKind::Whitespace, " ");
            b.start_node(SyntaxKind::LambdaExpr.into());
            tok(b, SyntaxKind::LBrace, "{");
            b.start_node(SyntaxKind::StringLit.into());
            tok(b, SyntaxKind::StringStart, "\"");
            tok(b, SyntaxKind::StringText, "a");
            tok(b, SyntaxKind::StringEscape, "\\n");
            tok(b, SyntaxKind::StringEscape, "{{");
            tok(b, SyntaxKind::StringText, "b ");
            b.start_node(SyntaxKind::Interpolation.into());
            tok(b, SyntaxKind::InterpStart, "{");
            b.start_node(SyntaxKind::NameExpr.into());
            tok(b, SyntaxKind::Ident, "count");
            b.finish_node();
            tok(b, SyntaxKind::InterpEnd, "}");
            b.finish_node();
            tok(b, SyntaxKind::StringEnd, "\"");
            b.finish_node();
            tok(b, SyntaxKind::RBrace, "}");
            b.finish_node();
            b.finish_node();
            b.finish_node();
        });
        assert_eq!(
            dump_tree(&root),
            "(SourceFile\n  (CallExpr (NameExpr text) (LambdaExpr (StringLit \"a\\n{b \" (Interpolation (NameExpr count))))))\n"
        );
    }

    #[test]
    fn error_tokens_with_unsafe_text_are_quoted() {
        let root = node(|b| {
            b.start_node(SyntaxKind::SourceFile.into());
            tok(b, SyntaxKind::Error, "$");
            tok(b, SyntaxKind::Error, "a b");
            tok(b, SyntaxKind::QuestionDot, "?.");
            b.finish_node();
        });
        assert_eq!(dump_tree(&root), "(SourceFile $ \"a b\" ?.)\n");
    }

    #[test]
    fn escapes_decode() {
        let mut out = String::new();
        for e in [
            "\\n",
            "\\r",
            "\\t",
            "\\0",
            "\\\\",
            "\\\"",
            "{{",
            "\\u{41}",
            "\\u{1F600}",
            "\\q",
            "\\u{110000}",
            "\\u{}",
        ] {
            decode_escape(e, &mut out);
        }
        assert_eq!(out, "\n\r\t\0\\\"{A😀\\q\\u{110000}\\u{}");
    }
}
