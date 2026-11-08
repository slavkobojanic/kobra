//! The VS Code TextMate grammar (`editors/vscode/syntaxes/k.tmLanguage.json`)
//! tokenizes K the way the compiler's lexer does.
//!
//! The test runs the grammar with a small TextMate engine of its own (the
//! `begin`/`end`/`match`/`captures`/`include` subset the grammar uses, line by
//! line, earliest match first and the `end` pattern winning ties, as
//! `vscode-textmate` does). The grammar's regular expressions are written in
//! the subset that both Oniguruma and the `regex` crate understand (see
//! `tests/editors.rs`), so the engine evaluates them as VS Code would.
//!
//! Two kinds of check:
//!
//! * fixtures in `tests/textmate/` with scopes asserted at given places (the
//!   test names of `Panel.test.k` were once highlighted as code, and their
//!   quotes paired up across lines);
//! * an oracle: for every `.k` file of the examples, the standard library and
//!   the lexer-clean corpus, every string token of `k_syntax::lex` is inside a
//!   grammar string, every comment token inside a grammar comment and every
//!   other token in neither.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use k_diagnostics::FileId;
use k_syntax::SyntaxKind;
use regex::Regex;
use serde_json::Value;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ------------------------------------------------------------------ engine

/// The grammar and a cache of its compiled regexes.
struct Grammar {
    /// Leaked, so rules can be borrowed for `'static` by open frames.
    root: &'static Value,
    regexes: RefCell<HashMap<String, Regex>>,
    /// The candidates of each open rule (by address; `0` for the top level).
    candidates: RefCell<HashMap<usize, Rc<Candidates>>>,
}

/// What can match next: the open rule's `end` first, then its patterns.
type Candidates = Vec<(Candidate, Regex)>;

/// A begin/end rule that is open, with the scopes it adds.
struct Frame {
    rule: &'static Value,
    /// The enclosing scopes plus the rule's `name`.
    scopes: Vec<String>,
    /// `scopes` plus the rule's `contentName`.
    content: Vec<String>,
}

/// A run of text with one scope stack, at byte offsets of the whole file.
#[derive(Debug)]
struct Token {
    start: usize,
    end: usize,
    scopes: Vec<String>,
}

#[derive(Clone)]
enum Candidate {
    End,
    Rule(&'static Value),
}

impl Grammar {
    fn load() -> Grammar {
        let path = repo().join("editors/vscode/syntaxes/k.tmLanguage.json");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        Grammar {
            root: Box::leak(Box::new(serde_json::from_str(&text).unwrap())),
            regexes: RefCell::new(HashMap::new()),
            candidates: RefCell::new(HashMap::new()),
        }
    }

    fn regex(&self, source: &str) -> Regex {
        self.regexes
            .borrow_mut()
            .entry(source.to_owned())
            .or_insert_with(|| {
                Regex::new(source).unwrap_or_else(|e| panic!("{source:?} doesn't compile: {e}"))
            })
            .clone()
    }

    /// The candidates while `open` is the innermost open rule (`None` at the
    /// top level), cached by the rule's address.
    fn candidates(&self, open: Option<&'static Value>, patterns: &'static Value) -> Rc<Candidates> {
        let key = open.map_or(0, |rule| std::ptr::from_ref(rule) as usize);
        if let Some(found) = self.candidates.borrow().get(&key) {
            return Rc::clone(found);
        }
        let mut list = Vec::new();
        if let Some(rule) = open {
            list.push((Candidate::End, self.regex(rule["end"].as_str().unwrap())));
        }
        let mut rules = Vec::new();
        self.flatten(patterns, &mut rules, 0);
        for rule in rules {
            let source = rule
                .get("match")
                .or_else(|| rule.get("begin"))
                .and_then(Value::as_str)
                .unwrap();
            list.push((Candidate::Rule(rule), self.regex(source)));
        }
        let list = Rc::new(list);
        self.candidates.borrow_mut().insert(key, Rc::clone(&list));
        list
    }

    /// The `match` and `begin` rules a list of patterns stands for, includes
    /// resolved, in order.
    fn flatten(&self, patterns: &'static Value, out: &mut Vec<&'static Value>, depth: usize) {
        assert!(depth < 32, "the grammar's includes recurse");
        for pattern in patterns.as_array().into_iter().flatten() {
            self.flatten_one(pattern, out, depth);
        }
    }

    fn flatten_one(&self, pattern: &'static Value, out: &mut Vec<&'static Value>, depth: usize) {
        if let Some(include) = pattern.get("include").and_then(Value::as_str) {
            let target = match include {
                "$self" | "$base" => {
                    return self.flatten(&self.root["patterns"], out, depth + 1);
                }
                name => {
                    let key = name.strip_prefix('#').unwrap_or_else(|| {
                        panic!("include {name:?} is not a repository reference")
                    });
                    self.root["repository"]
                        .get(key)
                        .unwrap_or_else(|| panic!("include {name:?} names no repository entry"))
                }
            };
            self.flatten_one(target, out, depth + 1);
        } else if pattern.get("match").is_some() || pattern.get("begin").is_some() {
            out.push(pattern);
        } else if let Some(patterns) = pattern.get("patterns") {
            self.flatten(patterns, out, depth + 1);
        }
    }

    /// Tokenizes `text` line by line, carrying open begin/end rules across
    /// lines.
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut stack: Vec<Frame> = Vec::new();
        // Lines end at `\n`, `\r\n` or a lone `\r`, as in the lexer and VS Code.
        let bytes = text.as_bytes();
        let mut line_start = 0;
        let mut i = 0;
        while i <= bytes.len() {
            if i == bytes.len() || matches!(bytes[i], b'\n' | b'\r') {
                self.tokenize_line(&text[line_start..i], line_start, &mut stack, &mut tokens);
                if i + 1 < bytes.len() && bytes[i] == b'\r' && bytes[i + 1] == b'\n' {
                    i += 1;
                }
                line_start = i + 1;
            }
            i += 1;
        }
        tokens
    }

    fn tokenize_line(
        &self,
        line: &str,
        offset: usize,
        stack: &mut Vec<Frame>,
        tokens: &mut Vec<Token>,
    ) {
        let mut pos = 0;
        let mut stalled = 0;
        // Each regex's last search on this line: from where, and its match.
        // A match found from `p` that starts at or after `pos >= p` is also
        // the leftmost from `pos`, and no match from `p` means none from `pos`.
        let mut searched: HashMap<usize, (usize, Option<usize>)> = HashMap::new();
        loop {
            let (patterns, scopes) = match stack.last() {
                Some(frame) => (&frame.rule["patterns"], frame.content.clone()),
                None => (&self.root["patterns"], vec!["source.k".to_owned()]),
            };
            let candidates = self.candidates(stack.last().map(|f| f.rule), patterns);
            // The earliest match wins; on a tie, the first candidate.
            let mut best: Option<(usize, usize)> = None;
            if pos <= line.len() {
                for (index, (_, regex)) in candidates.iter().enumerate() {
                    let key = regex.as_str().as_ptr() as usize;
                    let start = match searched.get(&key) {
                        Some(&(from, found))
                            if from <= pos && found.is_none_or(|start| start >= pos) =>
                        {
                            found
                        }
                        _ => {
                            let found = regex.find_at(line, pos).map(|m| m.start());
                            searched.insert(key, (pos, found));
                            found
                        }
                    };
                    if let Some(start) = start
                        && best.is_none_or(|(_, b)| start < b)
                    {
                        best = Some((index, start));
                    }
                }
            }
            let best = best.map(|(index, _)| {
                let (candidate, regex) = &candidates[index];
                (candidate.clone(), regex.captures_at(line, pos).unwrap())
            });
            let Some((candidate, caps)) = best else {
                push(tokens, offset + pos, offset + line.len(), &scopes);
                return;
            };
            let whole = caps.get(0).unwrap();
            push(tokens, offset + pos, offset + whole.start(), &scopes);
            match candidate {
                Candidate::End => {
                    let frame = stack.pop().unwrap();
                    let captures = frame
                        .rule
                        .get("endCaptures")
                        .or_else(|| frame.rule.get("captures"));
                    emit(&caps, captures, &frame.scopes, offset, tokens);
                }
                Candidate::Rule(rule) => {
                    let mut inner = scopes.clone();
                    if let Some(name) = rule.get("name").and_then(Value::as_str) {
                        inner.push(name.to_owned());
                    }
                    if rule.get("begin").is_some() {
                        let captures = rule.get("beginCaptures").or_else(|| rule.get("captures"));
                        emit(&caps, captures, &inner, offset, tokens);
                        let mut content = inner.clone();
                        if let Some(name) = rule.get("contentName").and_then(Value::as_str) {
                            content.push(name.to_owned());
                        }
                        stack.push(Frame {
                            rule,
                            scopes: inner,
                            content,
                        });
                    } else {
                        emit(&caps, rule.get("captures"), &inner, offset, tokens);
                    }
                }
            }
            if whole.end() == pos && whole.start() == pos {
                stalled += 1;
                assert!(stalled < 64, "the grammar loops at {line:?}:{pos}");
            } else {
                stalled = 0;
            }
            pos = whole.end();
            if pos >= line.len() && stack.is_empty() {
                return;
            }
        }
    }
}

fn push(tokens: &mut Vec<Token>, start: usize, end: usize, scopes: &[String]) {
    if start < end {
        tokens.push(Token {
            start,
            end,
            scopes: scopes.to_vec(),
        });
    }
}

/// Emits a match: its numbered captures with their names, the gaps between
/// them with `scopes` alone.
fn emit(
    caps: &regex::Captures<'_>,
    captures: Option<&Value>,
    scopes: &[String],
    offset: usize,
    tokens: &mut Vec<Token>,
) {
    let whole = caps.get(0).unwrap();
    let mut named: Vec<(usize, usize, Vec<String>)> = Vec::new();
    let mut whole_scopes = scopes.to_vec();
    if let Some(Value::Object(map)) = captures {
        for (index, capture) in map {
            let index: usize = index.parse().unwrap();
            let Some(name) = capture.get("name").and_then(Value::as_str) else {
                continue;
            };
            if index == 0 {
                whole_scopes.push(name.to_owned());
            } else if let Some(m) = caps.get(index) {
                named.push((m.start(), m.end(), vec![name.to_owned()]));
            }
        }
    }
    named.sort_by_key(|n| n.0);
    let mut at = whole.start();
    for (start, end, extra) in named {
        push(tokens, offset + at, offset + start, &whole_scopes);
        let mut s = whole_scopes.clone();
        s.extend(extra);
        push(tokens, offset + start, offset + end, &s);
        at = end;
    }
    push(tokens, offset + at, offset + whole.end(), &whole_scopes);
}

/// The scopes at byte `at`.
fn scopes_at(tokens: &[Token], at: usize) -> &[String] {
    let index = tokens.partition_point(|t| t.end <= at);
    let token = &tokens[index];
    assert!(token.start <= at && at < token.end, "no token at {at}");
    &token.scopes
}

/// What a scope stack ends in: the innermost string, interpolation or
/// comment scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Code,
    String,
    Comment,
}

fn region(scopes: &[String]) -> Region {
    for scope in scopes.iter().rev() {
        if scope.starts_with("string.") {
            return Region::String;
        }
        if scope.starts_with("comment.") {
            return Region::Comment;
        }
        if scope.starts_with("meta.interpolation") {
            return Region::Code;
        }
    }
    Region::Code
}

// ---------------------------------------------------------------- fixtures

struct Fixture {
    text: String,
    tokens: Vec<Token>,
}

impl Fixture {
    fn load(grammar: &Grammar, name: &str) -> Fixture {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/textmate")
            .join(name);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let tokens = grammar.tokenize(&text);
        Fixture { text, tokens }
    }

    /// The offset of `needle` within the first line containing `line`.
    fn find(&self, line: &str, needle: &str) -> usize {
        let line_at = self
            .text
            .find(line)
            .unwrap_or_else(|| panic!("no line {line:?}"));
        let within = self.text[line_at..]
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle:?} after {line:?}"));
        line_at + within
    }

    /// The scopes of `needle`'s first byte, on the line containing `line`.
    fn scopes(&self, line: &str, needle: &str) -> &[String] {
        scopes_at(&self.tokens, self.find(line, needle))
    }

    /// Asserts that every byte of `needle` (on the line containing `line`)
    /// is in `expected` and carries no scope starting with any of `not`.
    fn assert(&self, line: &str, needle: &str, expected: Region, not: &[&str]) {
        let at = self.find(line, needle);
        for byte in at..at + needle.len() {
            let scopes = scopes_at(&self.tokens, byte);
            assert_eq!(
                region(scopes),
                expected,
                "{needle:?} in {line:?}: {scopes:?}"
            );
            for prefix in not {
                assert!(
                    !scopes.iter().any(|s| s.starts_with(prefix)),
                    "{needle:?} in {line:?} is {prefix}: {scopes:?}"
                );
            }
        }
    }

    fn has(&self, line: &str, needle: &str, scope: &str) -> bool {
        self.scopes(line, needle).iter().any(|s| s == scope)
    }
}

const KEYWORDISH: &[&str] = &["keyword", "storage", "constant", "entity", "variable"];

#[test]
fn test_names_are_one_string() {
    let grammar = Grammar::load();
    let f = Fixture::load(&grammar, "Panel.test.k");
    let first = "test \"by default, the panel is a level 2 heading and its content\" {";
    assert!(f.has(first, "test", "storage.type.test.k"));
    f.assert(
        first,
        "\"by default, the panel is a level 2 heading and its content\"",
        Region::String,
        KEYWORDISH,
    );
    assert!(f.has(first, "\"by", "string.quoted.double.k"));
    assert_eq!(region(f.scopes(first, "{")), Region::Code);

    // An apostrophe is text in a string, and the quotes pair up on the line.
    let second = "test \"the heading level can be set for a page's main panel\" {";
    f.assert(
        second,
        "\"the heading level can be set for a page's main panel\"",
        Region::String,
        KEYWORDISH,
    );
    f.assert(second, " {", Region::Code, &["string", "invalid"]);
    assert!(f.has(
        "expect screen.heading(named: \"Groceries\").level == 1",
        "expect",
        "keyword.control.k"
    ));
    assert!(f.has(
        "expect screen.heading(named: \"Groceries\").level == 1",
        "1",
        "constant.numeric.k"
    ));

    // The suite's closing brace is plain code, as is every line ending.
    let last = f.text.trim_end().len() - 1;
    let scopes = scopes_at(&f.tokens, last);
    assert_eq!(region(scopes), Region::Code, "{scopes:?}");
    assert!(!scopes.iter().any(|s| s.starts_with("invalid")));
    assert!(f.has("test Panel {", "Panel", "entity.name.type.k"));
}

#[test]
fn strings_interpolations_and_comments_are_scoped() {
    let grammar = Grammar::load();
    let f = Fixture::load(&grammar, "audit.k");

    // Doc comments, with code spans and links; quotes and braces are text.
    let doc = "/// - Parameter title";
    f.assert(
        doc,
        "\"quoted\" and {braced}",
        Region::Comment,
        &["string", "meta"],
    );
    assert!(f.has(doc, "Parameter", "keyword.other.documentation.k"));
    assert!(f.has("/// A board", "[`Card`]", "markup.underline.link.k"));
    f.assert(
        "// A line comment",
        "quote \" and an apostrophe '",
        Region::Comment,
        &["string"],
    );
    f.assert(
        "/* A block comment",
        "with \"quotes\" */",
        Region::Comment,
        &["string"],
    );

    // Annotations, generics and constraints.
    assert!(f.has("@Env", "Env", "storage.type.annotation.k"));
    f.assert("@Env", "\"BOARD_TITLE\"", Region::String, &[]);
    assert!(f.has("@Env", "Secret", "entity.name.type.k"));
    let generic = "public struct Box<T>";
    assert!(f.has(generic, "Box", "entity.name.type.k"));
    assert!(f.has(generic, "implements", "storage.modifier.k"));
    assert!(f.has(generic, "where", "storage.modifier.k"));
    assert!(f.has(generic, "Equatable", "entity.name.type.k"));

    // Escapes, `{{`, and words that are keywords elsewhere.
    let doubled = "let doubled";
    assert!(f.has(doubled, "{{", "constant.character.escape.k"));
    f.assert(doubled, "literal}} and ", Region::String, KEYWORDISH);
    assert!(f.has(doubled, "\\\"", "constant.character.escape.k"));
    assert!(f.has(doubled, "\\u{1F600}", "constant.character.escape.k"));

    // An interpolation holding braces of its own ends at its own `}`.
    let nested = "let nested";
    assert!(f.has(
        nested,
        "{ items",
        "punctuation.section.interpolation.begin.k"
    ));
    assert!(f.has(nested, "map", "variable.other.member.k"));
    assert!(f.has(nested, "=>", "keyword.operator.arrow.k"));
    f.assert(nested, "item.count }", Region::Code, &[]);
    assert!(f.has(nested, "} left", "punctuation.section.interpolation.end.k"));
    f.assert(nested, " left, and is 2\"", Region::String, KEYWORDISH);

    // A string in an interpolation in a string.
    let inner = "let inner";
    f.assert(inner, "\"inner ", Region::String, KEYWORDISH);
    f.assert(inner, "count}", Region::Code, &[]);
    f.assert(inner, " words\"", Region::String, &[]);
    f.assert(inner, " done\"", Region::String, &[]);

    // `"{x} cards"`.
    let cards = "return \"{count} cards\"";
    f.assert(cards, "count}", Region::Code, &[]);
    f.assert(cards, " cards\"", Region::String, &[]);

    // View and style blocks.
    assert!(f.has("    view {", "view", "keyword.other.block.k"));
    f.assert("heading {", "\"The board's ", Region::String, KEYWORDISH);
    f.assert("heading {", "title}", Region::Code, &["invalid"]);
    f.assert("heading {", " cards\"", Region::String, &[]);
    let close = f.find("heading {", "cards\" }") + "cards\" ".len();
    assert_eq!(region(scopes_at(&f.tokens, close)), Region::Code);
    assert!(f.has("    style {", "style", "keyword.other.block.k"));
    assert!(f.has(".card {", "muted", "variable.other.member.k"));
    assert!(f.has(".card {", "px", "keyword.other.unit.k"));

    // Test declarations: a name, a string, nested.
    let named = "test \"if the board is empty, it shows a default message\"";
    f.assert(named, &named[5..], Region::String, KEYWORDISH);
    assert!(f.has("    expect label", "expect", "keyword.control.k"));
    assert!(f.has("test Board {", "Board", "entity.name.type.k"));
    f.assert(
        "test \"it doesn't crash\"",
        "\"it doesn't crash\"",
        Region::String,
        KEYWORDISH,
    );
    assert!(f.has("        expect true", "true", "constant.language.k"));
}

#[test]
fn a_string_ends_at_the_end_of_its_line() {
    // A string cannot span lines (grammar.md §4.2): an unterminated one, or
    // one with an unclosed interpolation, does not colour the next line.
    let grammar = Grammar::load();
    for text in [
        "let a = \"open\nlet b = 1\n",
        "let a = \"open {x\nlet b = 1\n",
        "let a = \"open {x {y\nlet b = 1\n",
    ] {
        let tokens = grammar.tokenize(text);
        let at = text.find("let b").unwrap();
        let scopes = scopes_at(&tokens, at);
        assert_eq!(region(scopes), Region::Code, "{text:?}: {scopes:?}");
        assert!(
            scopes.iter().any(|s| s == "storage.type.k"),
            "{text:?}: {scopes:?}"
        );
        assert_eq!(scopes.len(), 2, "{text:?}: {scopes:?}");
    }
}

// ------------------------------------------------------------------ oracle

fn k_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !matches!(name, "target" | "node_modules" | "dist" | ".k") {
                k_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "k") {
            out.push(path);
        }
    }
}

/// The region the lexer puts a token in, or `None` for trivia and errors.
fn lexer_region(kind: SyntaxKind) -> Option<Region> {
    match kind {
        SyntaxKind::StringStart
        | SyntaxKind::StringText
        | SyntaxKind::StringEscape
        | SyntaxKind::StringEnd => Some(Region::String),
        SyntaxKind::LineComment | SyntaxKind::BlockComment | SyntaxKind::DocComment => {
            Some(Region::Comment)
        }
        SyntaxKind::Whitespace | SyntaxKind::Newline | SyntaxKind::Error => None,
        _ => Some(Region::Code),
    }
}

#[test]
fn the_grammar_agrees_with_the_lexer_on_strings_and_comments() {
    let grammar = Grammar::load();
    let root = repo();
    let mut files = Vec::new();
    for dir in ["examples", "crates/k_sema/stdlib", "tests/corpus"] {
        k_files(&root.join(dir), &mut files);
    }
    k_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/textmate"),
        &mut files,
    );
    assert!(files.len() > 100, "found only {} .k files", files.len());
    let mut checked = 0;
    let mut failures = Vec::new();
    for path in &files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let lexed = k_syntax::lex(FileId::new(0), &text);
        // Files the lexer rejects are recovery tests, not K as written.
        if !lexed.diagnostics().is_empty() {
            continue;
        }
        checked += 1;
        let tokens = grammar.tokenize(&text);
        let mut at = 0;
        for (kind, piece) in lexed.iter() {
            if let Some(expected) = lexer_region(kind)
                && !piece.trim().is_empty()
            {
                let first = at + (piece.len() - piece.trim_start().len());
                let scopes = scopes_at(&tokens, first);
                if region(scopes) != expected {
                    let line = text[..first].matches('\n').count() + 1;
                    failures.push(format!(
                        "{}:{line}: {kind:?} {piece:?} is {:?}, not {expected:?} ({scopes:?})",
                        path.strip_prefix(&root).unwrap_or(path).display(),
                        region(scopes),
                    ));
                }
            }
            at += piece.len();
        }
    }
    assert!(checked > 100, "checked only {checked} files");
    assert!(
        failures.is_empty(),
        "{} tokens the grammar scopes differently from the lexer:\n{}",
        failures.len(),
        failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
