//! A comment and whitespace stripper for the hand-written runtime.
//!
//! It is deliberately not a minifier: it never renames, never reorders and
//! never removes a line break between two tokens, so automatic semicolon
//! insertion sees the same program. It removes comments, indentation,
//! trailing spaces and blank lines, and collapses runs of spaces outside
//! literals to one.
//!
//! The runtime sources follow one rule that makes this safe without a full
//! tokenizer: **they contain no regular-expression literals** (they use
//! `new RegExp(…)`), so every `/` outside a string, template or comment is
//! a division operator or starts a comment. [`compact`] reports a `/` that
//! could start a regular expression as [`CompactError::RegexLiteral`], and a
//! test runs it over every runtime file.

use std::fmt;

/// Why [`compact`] refused a source.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CompactError {
    /// A string, template literal or block comment is not closed at the end
    /// of the source. `offset` is the byte offset where it starts.
    Unterminated {
        /// The byte offset of the opening quote, backtick or `/*`.
        offset: usize,
    },
    /// A `/` at `offset` is in a position where it would start a regular
    /// expression literal, which runtime sources don't use.
    RegexLiteral {
        /// The byte offset of the `/`.
        offset: usize,
    },
}

impl fmt::Display for CompactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompactError::Unterminated { offset } => {
                write!(f, "unterminated literal or comment at byte {offset}")
            }
            CompactError::RegexLiteral { offset } => write!(
                f,
                "a regular-expression literal at byte {offset}; runtime sources use `new RegExp(...)`"
            ),
        }
    }
}

impl std::error::Error for CompactError {}

/// Strips comments and redundant whitespace from runtime JavaScript.
///
/// The output keeps every token and every line break that separates two
/// tokens; string and template literals (including their `${…}`
/// expressions, which are compacted recursively) are copied exactly. The
/// result ends with a newline unless it is empty.
///
/// # Errors
///
/// [`CompactError::Unterminated`] for a literal or block comment that runs
/// to the end of the input, [`CompactError::RegexLiteral`] for a `/` that
/// would start a regular expression. Never panics.
pub fn compact(source: &str) -> Result<String, CompactError> {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len() / 2);
    // For each open template literal, the brace depth of its `${` code.
    let mut templates: Vec<u32> = Vec::new();
    let mut braces: u32 = 0;
    let mut pending_space = false;
    let mut pending_newline = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\r' => {
                pending_space = true;
                i += 1;
            }
            b'\n' => {
                pending_newline = true;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let start = i;
                let Some(end) = source[i + 2..].find("*/") else {
                    return Err(CompactError::Unterminated { offset: start });
                };
                let comment = &source[i..i + 2 + end + 2];
                if comment.contains('\n') {
                    pending_newline = true;
                } else {
                    pending_space = true;
                }
                i += 2 + end + 2;
            }
            _ => {
                if c == b'/' && regex_position(&out) {
                    return Err(CompactError::RegexLiteral { offset: i });
                }
                flush_space(&mut out, &mut pending_space, &mut pending_newline);
                match c {
                    b'"' | b'\'' => {
                        let end = string_end(bytes, i, c)
                            .ok_or(CompactError::Unterminated { offset: i })?;
                        out.push_str(&source[i..end]);
                        i = end;
                    }
                    b'`' => {
                        let end = template_chunk_end(bytes, i + 1)
                            .ok_or(CompactError::Unterminated { offset: i })?;
                        out.push_str(&source[i..end.0]);
                        i = end.0;
                        if end.1 {
                            templates.push(braces);
                            braces += 1;
                        }
                    }
                    b'{' => {
                        braces += 1;
                        out.push('{');
                        i += 1;
                    }
                    b'}' => {
                        braces = braces.saturating_sub(1);
                        if templates.last() == Some(&braces) {
                            // The `}` closes a `${…}`: continue the template.
                            templates.pop();
                            let end = template_chunk_end(bytes, i + 1)
                                .ok_or(CompactError::Unterminated { offset: i })?;
                            out.push_str(&source[i..end.0]);
                            i = end.0;
                            if end.1 {
                                templates.push(braces);
                                braces += 1;
                            }
                        } else {
                            out.push('}');
                            i += 1;
                        }
                    }
                    _ => {
                        let ch = source[i..].chars().next().unwrap_or(' ');
                        out.push(ch);
                        i += ch.len_utf8();
                    }
                }
            }
        }
    }
    if !templates.is_empty() {
        return Err(CompactError::Unterminated {
            offset: source.len(),
        });
    }
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

/// Emits the whitespace pending before a token: a newline if any line break
/// was seen (and something was emitted before), otherwise one space when it
/// separates two identifier-like characters or would otherwise merge
/// tokens.
fn flush_space(out: &mut String, space: &mut bool, newline: &mut bool) {
    if out.is_empty() {
        *space = false;
        *newline = false;
        return;
    }
    if *newline {
        out.push('\n');
    } else if *space {
        out.push(' ');
    }
    *space = false;
    *newline = false;
}

/// Returns `true` if a `/` after `out` would start a regular expression:
/// after an operator, an opening bracket, a comma, a semicolon, or one of
/// the keywords after which an expression starts.
fn regex_position(out: &str) -> bool {
    let trimmed = out.trim_end();
    let Some(last) = trimmed.chars().last() else {
        return true;
    };
    if "(,=:[!&|?{};+-*%<>~^".contains(last) {
        return true;
    }
    let word: String = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    matches!(
        word.as_str(),
        "return"
            | "typeof"
            | "instanceof"
            | "in"
            | "of"
            | "new"
            | "delete"
            | "void"
            | "throw"
            | "case"
            | "do"
            | "else"
            | "yield"
            | "await"
    )
}

/// The byte offset just past the closing `quote` of a string starting at
/// `start`, honouring backslash escapes; `None` if a line ends first.
fn string_end(bytes: &[u8], start: usize, quote: u8) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' => return None,
            b if b == quote => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// Scans template text from `i` to the closing backtick or the next `${`.
/// Returns the offset just past it and `true` if it was `${`.
fn template_chunk_end(bytes: &[u8], mut i: usize) -> Option<(usize, bool)> {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'`' => return Some((i + 1, false)),
            b'$' if bytes.get(i + 1) == Some(&b'{') => return Some((i + 2, true)),
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> String {
        compact(s).expect("compacts")
    }

    #[test]
    fn strips_line_and_block_comments() {
        assert_eq!(ok("a; // x\n/** doc\n * more */\nb;"), "a;\nb;\n");
        assert_eq!(ok("a /* inline */ + b"), "a + b\n");
    }

    #[test]
    fn removes_indentation_and_blank_lines_but_keeps_line_breaks() {
        assert_eq!(ok("  if (x) {\n\n    y()\n  }\n"), "if (x) {\ny()\n}\n");
    }

    #[test]
    fn copies_strings_exactly() {
        assert_eq!(
            ok("f('a // b', \"c /* d */\", 'e\\'f')"),
            "f('a // b', \"c /* d */\", 'e\\'f')\n"
        );
    }

    #[test]
    fn copies_templates_and_compacts_their_expressions() {
        assert_eq!(
            ok("`a ${ x  /* c */ + `n ${y}` } // b`"),
            "`a ${ x + `n ${y}` } // b`\n"
        );
        assert_eq!(ok("`${ {a: 1}.a }`"), "`${ {a: 1}.a }`\n");
    }

    #[test]
    fn keeps_division() {
        assert_eq!(ok("a / b / c"), "a / b / c\n");
        assert_eq!(ok("x = (a) / 2"), "x = (a) / 2\n");
    }

    #[test]
    fn rejects_regex_literals() {
        assert_eq!(
            compact("x = /ab/g"),
            Err(CompactError::RegexLiteral { offset: 4 })
        );
        assert!(matches!(
            compact("return /a/.test(s)"),
            Err(CompactError::RegexLiteral { .. })
        ));
        assert!(matches!(
            compact("/a/"),
            Err(CompactError::RegexLiteral { .. })
        ));
    }

    #[test]
    fn reports_unterminated_input() {
        assert_eq!(
            compact("'abc"),
            Err(CompactError::Unterminated { offset: 0 })
        );
        assert_eq!(
            compact("a /* b"),
            Err(CompactError::Unterminated { offset: 2 })
        );
        assert!(matches!(
            compact("`a ${b"),
            Err(CompactError::Unterminated { .. })
        ));
        assert!(matches!(
            compact("`abc"),
            Err(CompactError::Unterminated { .. })
        ));
    }

    #[test]
    fn empty_and_comment_only_inputs_compact_to_nothing() {
        assert_eq!(ok(""), "");
        assert_eq!(ok("// only\n/* c */\n"), "");
    }

    #[test]
    fn keeps_non_ascii_text() {
        assert_eq!(ok("const s = '✓';  é"), "const s = '✓'; é\n");
    }
}
