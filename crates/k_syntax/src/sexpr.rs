//! A minimal S-expression data model, reader and canonical printer.
//!
//! S-expressions are the format of `.ast` golden files (see
//! `docs/compiler/grammar.md`, "Syntax tree dump format"). The golden-test
//! harness reads both the expected file and the actual dump with [`parse`]
//! and compares the results structurally, so expected files may use any
//! layout and may contain `;` comments.
//!
//! # Syntax
//!
//! ```text
//! sexpr   = list | string | symbol
//! list    = "(" sexpr* ")"
//! string  = '"' ( char | escape )* '"'
//! escape  = "\\" | "\"" | "\n" | "\r" | "\t" | "\u{" hex+ "}"
//! symbol  = one or more characters other than whitespace, "(", ")", '"' and ";"
//! comment = ";" up to the end of the line
//! ```
//!
//! A symbol and a string with the same text are different values:
//! `name` is not `"name"`.

use std::fmt::{self, Write as _};

/// One S-expression.
///
/// Dumps of syntax trees can nest very deeply (one level per tree level), so
/// dropping, comparing and printing are implemented without recursion; only
/// [`Clone`] and [`fmt::Debug`] recurse.
#[derive(Clone, Debug)]
pub enum Sexpr {
    /// A bare atom such as `ComponentDecl`, `count` or `+=`.
    Symbol(String),
    /// A quoted atom such as `"Add"`; holds the decoded text.
    String(String),
    /// A parenthesized list.
    List(Vec<Sexpr>),
}

impl Sexpr {
    /// Creates an atom for `text`: a [`Sexpr::Symbol`] if the text can be
    /// written bare, otherwise a [`Sexpr::String`].
    #[must_use]
    pub fn atom(text: &str) -> Sexpr {
        if is_symbol_safe(text) {
            Sexpr::Symbol(text.to_owned())
        } else {
            Sexpr::String(text.to_owned())
        }
    }

    /// Renders this expression in canonical layout (see [`pretty`]).
    #[must_use]
    pub fn to_pretty(&self) -> String {
        pretty(self)
    }
}

impl Drop for Sexpr {
    /// Frees nested lists with an explicit stack instead of one native stack
    /// frame per level.
    fn drop(&mut self) {
        let Sexpr::List(items) = self else {
            return;
        };
        // Fast path: no child owns further items, so the default drop glue
        // does not recurse more than one level.
        if !items
            .iter()
            .any(|item| matches!(item, Sexpr::List(children) if !children.is_empty()))
        {
            return;
        }
        let mut stack = std::mem::take(items);
        while let Some(mut item) = stack.pop() {
            if let Sexpr::List(children) = &mut item {
                stack.append(children);
            }
            // `item` now owns no children, so dropping it does not recurse.
        }
    }
}

impl PartialEq for Sexpr {
    /// Structural equality, compared without recursion.
    fn eq(&self, other: &Sexpr) -> bool {
        let mut stack: Vec<(&Sexpr, &Sexpr)> = vec![(self, other)];
        while let Some((a, b)) = stack.pop() {
            match (a, b) {
                (Sexpr::Symbol(x), Sexpr::Symbol(y)) | (Sexpr::String(x), Sexpr::String(y)) => {
                    if x != y {
                        return false;
                    }
                }
                (Sexpr::List(x), Sexpr::List(y)) => {
                    if x.len() != y.len() {
                        return false;
                    }
                    stack.extend(x.iter().zip(y.iter()).rev());
                }
                _ => return false,
            }
        }
        true
    }
}

impl Eq for Sexpr {}

impl fmt::Display for Sexpr {
    /// Writes the expression on a single line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        write_flat(self, &mut out);
        f.write_str(&out)
    }
}

/// An error reading S-expression text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SexprError {
    /// 1-based line of the problem.
    pub line: u32,
    /// 1-based column (in chars) of the problem.
    pub col: u32,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for SexprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for SexprError {}

/// Returns `true` if `text` can be written as a bare symbol.
#[must_use]
pub fn is_symbol_safe(text: &str) -> bool {
    !text.is_empty()
        && !text
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';'))
}

/// Reads every S-expression in `text`.
///
/// # Errors
///
/// Returns an error for an unbalanced `)`, an unclosed list or string, or a
/// malformed escape.
pub fn parse(text: &str) -> Result<Vec<Sexpr>, SexprError> {
    let mut reader = Reader {
        chars: text.chars().peekable(),
        line: 1,
        col: 1,
    };
    let mut stack: Vec<(Vec<Sexpr>, u32, u32)> = Vec::new();
    let mut top = Vec::new();
    loop {
        reader.skip_space_and_comments();
        let (line, col) = (reader.line, reader.col);
        let Some(&c) = reader.chars.peek() else { break };
        match c {
            '(' => {
                reader.bump();
                stack.push((Vec::new(), line, col));
            }
            ')' => {
                reader.bump();
                let Some((items, _, _)) = stack.pop() else {
                    return Err(SexprError {
                        line,
                        col,
                        message: "unbalanced `)`".into(),
                    });
                };
                push(&mut stack, &mut top, Sexpr::List(items));
            }
            '"' => {
                let value = reader.string()?;
                push(&mut stack, &mut top, Sexpr::String(value));
            }
            _ => {
                let mut symbol = String::new();
                while let Some(&c) = reader.chars.peek() {
                    if c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';') {
                        break;
                    }
                    symbol.push(c);
                    reader.bump();
                }
                push(&mut stack, &mut top, Sexpr::Symbol(symbol));
            }
        }
    }
    if let Some((_, line, col)) = stack.pop() {
        return Err(SexprError {
            line,
            col,
            message: "unclosed `(`".into(),
        });
    }
    Ok(top)
}

fn push(stack: &mut [(Vec<Sexpr>, u32, u32)], top: &mut Vec<Sexpr>, value: Sexpr) {
    match stack.last_mut() {
        Some((items, _, _)) => items.push(value),
        None => top.push(value),
    }
}

struct Reader<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: u32,
    col: u32,
}

impl Reader<'_> {
    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_space_and_comments(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c == ';' {
                while let Some(&c) = self.chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.bump();
                }
            } else if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn string(&mut self) -> Result<String, SexprError> {
        let (line, col) = (self.line, self.col);
        self.bump();
        let mut value = String::new();
        loop {
            let (eline, ecol) = (self.line, self.col);
            match self.bump() {
                None => {
                    return Err(SexprError {
                        line,
                        col,
                        message: "unclosed string".into(),
                    });
                }
                Some('"') => return Ok(value),
                Some('\\') => {
                    let escaped = match self.bump() {
                        Some('\\') => '\\',
                        Some('"') => '"',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('u') => self.unicode_escape(eline, ecol)?,
                        _ => {
                            return Err(SexprError {
                                line: eline,
                                col: ecol,
                                message: "unknown escape".into(),
                            });
                        }
                    };
                    value.push(escaped);
                }
                Some(c) => value.push(c),
            }
        }
    }

    fn unicode_escape(&mut self, line: u32, col: u32) -> Result<char, SexprError> {
        let error = || SexprError {
            line,
            col,
            message: "malformed `\\u{...}` escape".into(),
        };
        if self.bump() != Some('{') {
            return Err(error());
        }
        let mut hex = String::new();
        loop {
            match self.bump() {
                Some('}') => break,
                Some(c) if c.is_ascii_hexdigit() && hex.len() < 6 => hex.push(c),
                _ => return Err(error()),
            }
        }
        u32::from_str_radix(&hex, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(error)
    }
}

/// Appends `text` to `out` as a quoted string with escapes, the inverse of
/// the string reader. Also used for token texts in `.tokens` files.
pub fn write_quoted(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The maximum line width of the canonical layout.
pub const PRETTY_WIDTH: usize = 100;

/// Renders `sexpr` in the canonical layout, ending with a newline.
///
/// A list that fits in [`PRETTY_WIDTH`] columns at its indentation is written
/// on one line. Otherwise its leading run of atoms stays on the first line
/// and every following element goes on its own line, indented two spaces
/// more than the list. This is the layout `K_BLESS=1` writes.
///
/// The flat width of every list is measured once, so the running time is
/// linear in the size of the output; neither pass recurses.
#[must_use]
pub fn pretty(sexpr: &Sexpr) -> String {
    let layout = Layout::measure(sexpr);
    let mut out = String::new();
    // The list currently being broken over lines: the elements still to be
    // written and the indentation of the list itself.
    let mut stack: Vec<(std::slice::Iter<'_, Sexpr>, usize)> = Vec::new();
    // The pre-order index of the next list to be visited.
    let mut cursor = 0usize;
    let mut next = Some((sexpr, 0usize));
    loop {
        if let Some((item, indent)) = next.take() {
            match item {
                Sexpr::List(items) if indent + layout.widths[cursor] > PRETTY_WIDTH => {
                    cursor += 1;
                    out.push('(');
                    let head = items
                        .iter()
                        .take_while(|item| !matches!(item, Sexpr::List(_)))
                        .count();
                    for (i, atom) in items[..head].iter().enumerate() {
                        if i > 0 {
                            out.push(' ');
                        }
                        write_flat(atom, &mut out);
                    }
                    stack.push((items[head..].iter(), indent));
                }
                Sexpr::List(_) => {
                    cursor = layout.ends[cursor];
                    write_flat(item, &mut out);
                }
                _ => write_flat(item, &mut out),
            }
        }
        let Some((rest, indent)) = stack.last_mut() else {
            break;
        };
        let indent = *indent;
        match rest.next() {
            Some(item) => {
                out.push('\n');
                out.extend(std::iter::repeat_n(' ', indent + 2));
                next = Some((item, indent + 2));
            }
            None => {
                out.push(')');
                stack.pop();
            }
        }
    }
    out.push('\n');
    out
}

/// The flat widths of the lists of one expression, in pre-order.
struct Layout {
    /// The width in chars of each list written on one line.
    widths: Vec<usize>,
    /// For each list, the pre-order index just past its last nested list.
    ends: Vec<usize>,
}

impl Layout {
    fn measure(root: &Sexpr) -> Layout {
        let mut layout = Layout {
            widths: Vec::new(),
            ends: Vec::new(),
        };
        let mut scratch = String::new();
        // Open lists: remaining elements, pre-order index, width so far.
        let mut stack: Vec<(std::slice::Iter<'_, Sexpr>, usize, usize)> = Vec::new();
        let mut next = Some(root);
        loop {
            // The width of a finished element, to be added to its parent.
            let mut finished = None;
            if let Some(item) = next.take() {
                match item {
                    Sexpr::List(items) => {
                        let index = layout.widths.len();
                        layout.widths.push(0);
                        layout.ends.push(0);
                        // `(` and `)` plus one space between elements.
                        let width = 2 + items.len().saturating_sub(1);
                        stack.push((items.iter(), index, width));
                    }
                    atom => {
                        scratch.clear();
                        write_flat(atom, &mut scratch);
                        finished = Some(scratch.chars().count());
                    }
                }
            }
            loop {
                let Some((rest, index, width)) = stack.last_mut() else {
                    return layout;
                };
                if let Some(child) = finished.take() {
                    *width += child;
                }
                if let Some(item) = rest.next() {
                    next = Some(item);
                    break;
                }
                let (index, width) = (*index, *width);
                stack.pop();
                layout.widths[index] = width;
                layout.ends[index] = layout.widths.len();
                finished = Some(width);
            }
        }
    }
}

/// Writes `sexpr` on a single line, without recursion.
fn write_flat(sexpr: &Sexpr, out: &mut String) {
    // Open lists: the elements still to be written, and whether the next
    // one is the first (which is not preceded by a space).
    let mut stack: Vec<(std::slice::Iter<'_, Sexpr>, bool)> = Vec::new();
    let mut next = Some(sexpr);
    loop {
        match next.take() {
            Some(Sexpr::Symbol(text)) => {
                if is_symbol_safe(text) {
                    out.push_str(text);
                } else {
                    write_quoted(out, text);
                }
            }
            Some(Sexpr::String(text)) => write_quoted(out, text),
            Some(Sexpr::List(items)) => {
                out.push('(');
                stack.push((items.iter(), true));
            }
            None => {}
        }
        loop {
            let Some((rest, first)) = stack.last_mut() else {
                return;
            };
            if let Some(item) = rest.next() {
                if !*first {
                    out.push(' ');
                }
                *first = false;
                next = Some(item);
                break;
            }
            out.push(')');
            stack.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(s: &str) -> Sexpr {
        Sexpr::Symbol(s.into())
    }

    #[test]
    fn reads_nested_lists_strings_symbols_and_comments() {
        let text = "; leading comment\n(A b \"c d\" ; trailing\n  (E))\n(F)";
        let parsed = parse(text).unwrap();
        assert_eq!(
            parsed,
            vec![
                Sexpr::List(vec![
                    sym("A"),
                    sym("b"),
                    Sexpr::String("c d".into()),
                    Sexpr::List(vec![sym("E")])
                ]),
                Sexpr::List(vec![sym("F")]),
            ]
        );
    }

    #[test]
    fn symbols_may_contain_operator_characters() {
        let parsed = parse("(BinaryExpr a ..< b += ?. #3B82F6 16px 50% sans-serif)").unwrap();
        let Sexpr::List(items) = &parsed[0] else {
            panic!()
        };
        let texts: Vec<_> = items.iter().map(ToString::to_string).collect();
        assert_eq!(
            texts,
            [
                "BinaryExpr",
                "a",
                "..<",
                "b",
                "+=",
                "?.",
                "#3B82F6",
                "16px",
                "50%",
                "sans-serif"
            ]
        );
    }

    #[test]
    fn string_escapes_decode() {
        let parsed = parse(r#""a\"b\\c\nd\re\tf\u{1F600}""#).unwrap();
        assert_eq!(parsed, vec![Sexpr::String("a\"b\\c\nd\re\tf😀".into())]);
    }

    #[test]
    fn errors_have_positions() {
        assert_eq!(parse(")").unwrap_err().message, "unbalanced `)`");
        let e = parse("(a\n  (b)").unwrap_err();
        assert_eq!((e.line, e.col, e.message.as_str()), (1, 1, "unclosed `(`"));
        assert_eq!(parse("\"abc").unwrap_err().message, "unclosed string");
        assert_eq!(parse(r#""\q""#).unwrap_err().message, "unknown escape");
        assert!(parse(r#""\u{110000}""#).is_err());
        assert!(parse(r#""\u{}""#).is_err());
        assert!(parse(r#""\u12""#).is_err());
    }

    #[test]
    fn symbol_and_string_differ() {
        assert_ne!(parse("name").unwrap(), parse("\"name\"").unwrap());
    }

    #[test]
    fn atom_quotes_only_when_needed() {
        assert_eq!(Sexpr::atom("count"), sym("count"));
        assert_eq!(Sexpr::atom("a b"), Sexpr::String("a b".into()));
        assert_eq!(Sexpr::atom("("), Sexpr::String("(".into()));
        assert_eq!(Sexpr::atom(""), Sexpr::String(String::new()));
    }

    #[test]
    fn pretty_keeps_short_lists_flat() {
        let s = &parse("(FieldDecl (Annotation State) var count (PathType Int) = (Literal 0))")
            .unwrap()[0];
        assert_eq!(
            pretty(s),
            "(FieldDecl (Annotation State) var count (PathType Int) = (Literal 0))\n"
        );
    }

    #[test]
    fn pretty_breaks_long_lists_after_leading_atoms() {
        let long = "x".repeat(60);
        let text = format!("(Head a b (Child {long}) (Child {long}) tail)");
        let s = &parse(&text).unwrap()[0];
        let expected = format!("(Head a b\n  (Child {long})\n  (Child {long})\n  tail)\n");
        assert_eq!(pretty(s), expected);
        assert_eq!(&parse(&pretty(s)).unwrap()[0], s);
    }

    /// `depth` lists nested in each other, each with a leading atom.
    fn nested(depth: usize) -> Sexpr {
        let mut s = Sexpr::List(vec![sym("Leaf")]);
        for _ in 0..depth {
            s = Sexpr::List(vec![sym("Node"), s]);
        }
        s
    }

    #[test]
    fn deep_expressions_print_compare_and_drop_without_recursion() {
        // Far deeper than the default test-thread stack could recurse.
        let depth = 200_000;
        let a = nested(depth);
        let b = nested(depth);
        assert_eq!(a, b);
        assert_ne!(a, nested(depth - 1));
        let flat = a.to_string();
        assert_eq!(flat.len(), depth * "(Node ".len() + "(Leaf)".len() + depth);
        assert!(flat.starts_with("(Node (Node"), "{:.40}", flat);
        assert!(flat.trim_end_matches(')').ends_with("(Leaf"));
        assert_eq!(nested(2).to_string(), "(Node (Node (Leaf)))");
        drop(a);
        drop(b);
    }

    #[test]
    fn pretty_breaks_only_the_lists_that_do_not_fit() {
        // At nesting level k the list is indented 2k columns and is
        // 7 × (20 − k) + 6 columns wide flat, so levels 0 to 9 break and
        // level 10 (indent 20, width 76) is the first to fit on one line.
        let s = nested(20);
        let text = pretty(&s);
        assert_eq!(&parse(&text).unwrap()[0], &s);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 11, "{text}");
        assert_eq!(lines[9], format!("{}(Node", " ".repeat(18)));
        // The fit check counts the list itself, not the `)` of the broken
        // lists around it that end the same line.
        let last = format!("{}{}{}", " ".repeat(20), nested(10), ")".repeat(10));
        assert_eq!(lines[10], last);
    }

    /// The straightforward recursive layout that [`pretty`] must reproduce.
    fn reference_pretty(sexpr: &Sexpr, indent: usize, out: &mut String) {
        let flat = sexpr.to_string();
        let items = match sexpr {
            Sexpr::List(items) if indent + flat.chars().count() > PRETTY_WIDTH => items,
            _ => {
                out.push_str(&flat);
                return;
            }
        };
        out.push('(');
        let head = items
            .iter()
            .take_while(|item| !matches!(item, Sexpr::List(_)))
            .count();
        for (i, item) in items[..head].iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&item.to_string());
        }
        for item in &items[head..] {
            out.push('\n');
            out.extend(std::iter::repeat_n(' ', indent + 2));
            reference_pretty(item, indent + 2, out);
        }
        out.push(')');
    }

    #[test]
    fn pretty_matches_the_recursive_reference_layout() {
        // A deterministic family of shapes: wide and deep lists, atoms after
        // lists, quoted and non-ASCII atoms, empty lists.
        let mut seed = 0x2545_f491_u32;
        let mut next = move |n: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed % n
        };
        for _ in 0..300 {
            let mut stack = vec![vec![sym("Root")]];
            for _ in 0..next(120) {
                match next(6) {
                    0 | 1 => stack.push(vec![sym(["A", "Bb", "Ccc"][next(3) as usize])]),
                    2 if stack.len() > 1 => {
                        let done = Sexpr::List(stack.pop().unwrap());
                        stack.last_mut().unwrap().push(done);
                    }
                    3 => stack
                        .last_mut()
                        .unwrap()
                        .push(Sexpr::String("a b é".into())),
                    4 => stack
                        .last_mut()
                        .unwrap()
                        .push(sym(&"x".repeat(next(40) as usize + 1))),
                    _ => stack.last_mut().unwrap().push(Sexpr::List(Vec::new())),
                }
            }
            while stack.len() > 1 {
                let done = Sexpr::List(stack.pop().unwrap());
                stack.last_mut().unwrap().push(done);
            }
            let s = Sexpr::List(stack.pop().unwrap());
            let mut expected = String::new();
            reference_pretty(&s, 0, &mut expected);
            expected.push('\n');
            assert_eq!(pretty(&s), expected);
        }
    }

    #[test]
    fn quoted_round_trip_including_control_characters() {
        let original = "tab\there \"q\" back\\slash \u{1} \u{7f} é";
        let mut out = String::new();
        write_quoted(&mut out, original);
        assert_eq!(parse(&out).unwrap(), vec![Sexpr::String(original.into())]);
    }
}
