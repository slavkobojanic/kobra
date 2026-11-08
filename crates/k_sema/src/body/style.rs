//! Checking `style` blocks (sema.md §12.5): properties from the CSS table
//! (K3201), values against each property's value grammar (K3202), class
//! selectors, keyframes (K3205, K3207, K3208, K3213), at-rules (K3212),
//! media features (K3211) and `{ }` interpolations (K3210).
//!
//! A declaration's value is a list of components (keywords, literals, K
//! values, functions, `/` and `,`); the property's grammar, written in the
//! CSS value-definition syntax, is parsed once into a [`Grammar`] and matched
//! by [`Matcher`], which computes every end position a grammar can reach so
//! `||`, `&&` and multipliers need no special backtracking. When nothing
//! matches the whole value, the first component past the furthest match is
//! the one reported.

use std::collections::HashMap;

use k_diagnostics::{Diagnostic, Fix, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use super::{Checker, Target};
use crate::cx::{MemberFound, closest};
use crate::lookup::Resolution;
use crate::stdlib::css::{self, CssProperty};
use crate::ty::Ty;

/// One component of a declaration's value.
#[derive(Clone, Debug)]
enum Comp {
    /// A CSS keyword (`solid`, `spaceBetween`), in its camel-case K spelling.
    Keyword(String),
    /// A literal: its data-type kind.
    Literal(LitKind),
    /// A string literal.
    Str,
    /// A K value (an implicit member, a token, an interpolation) by its
    /// type's simple name; `None` for `{error}`, which matches anything.
    Value(Option<String>),
    /// A reference to keyframes of the block.
    Keyframes,
    /// A function with its arguments.
    Func(String, Vec<Comp>),
    /// `/`.
    Slash,
    /// `,` between comma-separated groups.
    Comma,
    /// Something already reported: matches anything.
    Any,
}

/// The kinds of literals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LitKind {
    /// An integer without unit.
    Int,
    /// `0`, which is also a length.
    Zero,
    /// A number with a fraction.
    Number,
    /// `16px`, `1rem`.
    Length,
    /// `50%`.
    Percent,
    /// `200ms`.
    Time,
    /// `45deg`.
    Angle,
    /// `1fr`.
    Flex,
    /// `#3B82F6`.
    Color,
}

/// A parsed value grammar.
#[derive(Clone, Debug)]
enum Grammar {
    /// A CSS keyword, matched literally.
    Keyword(String),
    /// A data type such as `<length>`.
    Type(String),
    /// Terms in order.
    Seq(Vec<Grammar>),
    /// Exactly one of the alternatives (`|`).
    OneOf(Vec<Grammar>),
    /// One or more of the items, each once, in any order (`||`).
    AnyOrder(Vec<Grammar>),
    /// All of the items, in any order (`&&`).
    All(Vec<Grammar>),
    /// Between `min` and `max` repetitions (`?`, `+`, `*`, `{m,n}`).
    Repeat(Box<Grammar>, usize, usize),
    /// A comma-separated list of one or more (`#`).
    CommaList(Box<Grammar>),
    /// A function such as `repeat( … )` with its argument grammar.
    Func(String, Box<Grammar>),
    /// A literal `/`.
    Slash,
    /// A literal `,` inside a function's arguments.
    Comma,
}

/// Parses a grammar in the CSS value-definition syntax.
struct GrammarParser {
    /// The grammar's tokens.
    tokens: Vec<String>,
    /// The next token's index.
    pos: usize,
}

impl GrammarParser {
    /// Splits a grammar into tokens: brackets, combinators, multipliers, data types, function starts (`name(`) and keywords.
    fn new(text: &str) -> GrammarParser {
        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            match c {
                '[' | ']' | '?' | '+' | '#' | '*' | ',' | '/' | ')' => {
                    tokens.push(c.to_string());
                    i += 1;
                }
                '|' | '&' => {
                    if chars.get(i + 1) == Some(&c) {
                        tokens.push(format!("{c}{c}"));
                        i += 2;
                    } else {
                        tokens.push(c.to_string());
                        i += 1;
                    }
                }
                '{' => {
                    let start = i;
                    while i < chars.len() && chars[i] != '}' {
                        i += 1;
                    }
                    i += 1;
                    tokens.push(chars[start..i.min(chars.len())].iter().collect());
                }
                '<' => {
                    let start = i;
                    while i < chars.len() && chars[i] != '>' {
                        i += 1;
                    }
                    i += 1;
                    tokens.push(chars[start..i.min(chars.len())].iter().collect());
                }
                _ => {
                    let start = i;
                    while i < chars.len()
                        && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_')
                    {
                        i += 1;
                    }
                    if i == start {
                        i += 1;
                        continue;
                    }
                    let mut word: String = chars[start..i].iter().collect();
                    if chars.get(i) == Some(&'(') {
                        word.push('(');
                        i += 1;
                    }
                    tokens.push(word);
                }
            }
        }
        GrammarParser { tokens, pos: 0 }
    }

    /// The next grammar token, if any.
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.pos).map(String::as_str)
    }

    /// Parses a whole grammar; malformed input yields the best grammar parsed so far (the tables are tested to parse cleanly).
    fn parse(text: &str) -> Grammar {
        let mut p = GrammarParser::new(text);
        p.one_of()
    }

    /// `a | b`: the lowest precedence.
    fn one_of(&mut self) -> Grammar {
        let mut alts = vec![self.any_order()];
        while self.peek() == Some("|") {
            self.pos += 1;
            alts.push(self.any_order());
        }
        if alts.len() == 1 {
            alts.remove(0)
        } else {
            Grammar::OneOf(alts)
        }
    }

    /// `a || b`: one or more, in any order.
    fn any_order(&mut self) -> Grammar {
        let mut items = vec![self.all()];
        while self.peek() == Some("||") {
            self.pos += 1;
            items.push(self.all());
        }
        if items.len() == 1 {
            items.remove(0)
        } else {
            Grammar::AnyOrder(items)
        }
    }

    /// `a && b`: all, in any order.
    fn all(&mut self) -> Grammar {
        let mut items = vec![self.seq()];
        while self.peek() == Some("&&") {
            self.pos += 1;
            items.push(self.seq());
        }
        if items.len() == 1 {
            items.remove(0)
        } else {
            Grammar::All(items)
        }
    }

    /// Juxtaposition: terms in order, the highest precedence.
    fn seq(&mut self) -> Grammar {
        let mut items = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(t, "|" | "||" | "&&" | "]" | ")") {
                break;
            }
            match self.term() {
                Some(g) => items.push(g),
                None => break,
            }
        }
        if items.len() == 1 {
            items.remove(0)
        } else {
            Grammar::Seq(items)
        }
    }

    /// One term with its multipliers (`?`, `+`, `*`, `#`, `{m,n}`).
    fn term(&mut self) -> Option<Grammar> {
        let t = self.peek()?.to_owned();
        self.pos += 1;
        let mut g = if t == "[" {
            let inner = self.one_of();
            if self.peek() == Some("]") {
                self.pos += 1;
            }
            inner
        } else if let Some(name) = t.strip_suffix('(') {
            let inner = self.one_of();
            if self.peek() == Some(")") {
                self.pos += 1;
            }
            Grammar::Func(name.to_owned(), Box::new(inner))
        } else if t.starts_with('<') {
            Grammar::Type(t)
        } else if t == "/" {
            Grammar::Slash
        } else if t == "," {
            Grammar::Comma
        } else {
            Grammar::Keyword(t)
        };
        // Multipliers.
        while let Some(m) = self.peek() {
            g = match m {
                "?" => Grammar::Repeat(Box::new(g), 0, 1),
                "+" => Grammar::Repeat(Box::new(g), 1, usize::MAX),
                "*" => Grammar::Repeat(Box::new(g), 0, usize::MAX),
                "#" => Grammar::CommaList(Box::new(g)),
                m if m.starts_with('{') => {
                    let inner = m.trim_start_matches('{').trim_end_matches('}');
                    let mut parts = inner.split(',');
                    let min = parts
                        .next()
                        .and_then(|n| n.trim().parse().ok())
                        .unwrap_or(1);
                    let max = parts
                        .next()
                        .map_or(min, |n| n.trim().parse().unwrap_or(usize::MAX));
                    Grammar::Repeat(Box::new(g), min, max)
                }
                _ => break,
            };
            self.pos += 1;
        }
        Some(g)
    }
}

/// Matches components against a grammar.
struct Matcher<'m> {
    /// The components being matched.
    comps: &'m [Comp],
    /// The furthest end position any terminal reached.
    far: usize,
}

impl Matcher<'_> {
    /// Every end position `g` can reach from `pos`.
    fn ends(&mut self, g: &Grammar, pos: usize) -> Vec<usize> {
        let mut out = Vec::new();
        match g {
            Grammar::Keyword(k) => {
                if matches!(self.comps.get(pos), Some(Comp::Keyword(w)) if w == k)
                    || matches!(self.comps.get(pos), Some(Comp::Any))
                {
                    out.push(pos + 1);
                }
            }
            Grammar::Type(t) => {
                if self.comps.get(pos).is_some_and(|c| satisfies(c, t)) {
                    out.push(pos + 1);
                }
            }
            Grammar::Slash => {
                if matches!(self.comps.get(pos), Some(Comp::Slash)) {
                    out.push(pos + 1);
                }
            }
            Grammar::Comma => {
                if matches!(self.comps.get(pos), Some(Comp::Comma)) {
                    out.push(pos + 1);
                }
            }
            Grammar::Func(name, inner) => {
                if let Some(Comp::Func(n, args)) = self.comps.get(pos) {
                    if n == name {
                        let mut m = Matcher {
                            comps: args,
                            far: 0,
                        };
                        if m.ends(inner, 0).contains(&args.len()) {
                            out.push(pos + 1);
                        }
                    }
                }
                if matches!(self.comps.get(pos), Some(Comp::Any)) {
                    out.push(pos + 1);
                }
            }
            Grammar::Seq(items) => {
                let mut current = vec![pos];
                for item in items {
                    let mut next = Vec::new();
                    for p in current {
                        for e in self.ends(item, p) {
                            if !next.contains(&e) {
                                next.push(e);
                            }
                        }
                    }
                    current = next;
                    if current.is_empty() {
                        break;
                    }
                }
                out = current;
            }
            Grammar::OneOf(alts) => {
                for a in alts {
                    for e in self.ends(a, pos) {
                        if !out.contains(&e) {
                            out.push(e);
                        }
                    }
                }
            }
            Grammar::AnyOrder(items) => self.unordered(items, 0, pos, false, &mut out),
            Grammar::All(items) => self.unordered(items, 0, pos, true, &mut out),
            Grammar::Repeat(inner, min, max) => {
                let mut current = vec![pos];
                if *min == 0 {
                    out.push(pos);
                }
                let mut count = 0;
                while count < *max && !current.is_empty() {
                    count += 1;
                    let mut next = Vec::new();
                    for p in &current {
                        for e in self.ends(inner, *p) {
                            if e > *p && !next.contains(&e) {
                                next.push(e);
                            }
                        }
                    }
                    if count >= *min {
                        for e in &next {
                            if !out.contains(e) {
                                out.push(*e);
                            }
                        }
                    }
                    current = next;
                }
            }
            Grammar::CommaList(inner) => {
                let mut current = self.ends(inner, pos);
                out.extend(current.iter().copied());
                while !current.is_empty() {
                    let mut next = Vec::new();
                    for p in &current {
                        if matches!(self.comps.get(*p), Some(Comp::Comma)) {
                            for e in self.ends(inner, p + 1) {
                                if !next.contains(&e) && !out.contains(&e) {
                                    next.push(e);
                                }
                            }
                        }
                    }
                    out.extend(next.iter().copied());
                    current = next;
                }
            }
        }
        for e in &out {
            self.far = self.far.max(*e);
        }
        out
    }

    /// `||` (any non-empty subset, any order) and `&&` (all, any order).
    fn unordered(
        &mut self,
        items: &[Grammar],
        used: u32,
        pos: usize,
        all: bool,
        out: &mut Vec<usize>,
    ) {
        for (i, item) in items.iter().enumerate() {
            let bit = 1u32 << i;
            if used & bit != 0 || i >= 31 {
                continue;
            }
            for e in self.ends(item, pos) {
                let mask = used | bit;
                let complete = mask.count_ones() as usize == items.len();
                if (!all || complete) && !out.contains(&e) {
                    out.push(e);
                }
                if !complete && e > pos {
                    self.unordered(items, mask, e, all, out);
                }
            }
        }
    }
}

/// Returns `true` if component `c` satisfies the data type `t`.
fn satisfies(c: &Comp, t: &str) -> bool {
    use LitKind as L;
    match c {
        Comp::Any => true,
        Comp::Keyword(k) => match t {
            "<custom-ident>" => true,
            _ => css::data_type_keywords(t).contains(&k.as_str()),
        },
        Comp::Literal(kind) => match t {
            "<number>" => matches!(kind, L::Int | L::Zero | L::Number),
            "<integer>" => matches!(kind, L::Int | L::Zero),
            "<length>" | "<line-width>" => matches!(kind, L::Length | L::Zero),
            "<length-percentage>" => matches!(kind, L::Length | L::Zero | L::Percent),
            "<percentage>" => *kind == L::Percent,
            "<time>" => *kind == L::Time,
            "<angle>" => matches!(kind, L::Angle | L::Zero),
            "<flex>" => *kind == L::Flex,
            "<color>" => *kind == L::Color,
            _ => false,
        },
        Comp::Str => t == "<string>",
        Comp::Keyframes => t == "<keyframes-name>",
        Comp::Value(name) => {
            let Some(name) = name else { return true };
            css::data_type(t).is_some_and(|dt| dt.types.contains(&name.as_str()))
        }
        Comp::Func(..) | Comp::Slash | Comp::Comma => false,
    }
}

/// The data-type kind of a literal token.
fn literal_kind(token: &SyntaxToken) -> LitKind {
    let text = token.text();
    match token.kind() {
        SyntaxKind::ColorLit => LitKind::Color,
        SyntaxKind::IntLit => {
            if text.trim_start_matches('0').is_empty() {
                LitKind::Zero
            } else {
                LitKind::Int
            }
        }
        SyntaxKind::FloatLit => LitKind::Number,
        _ => {
            let unit: String = text
                .chars()
                .skip_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '_')
                .collect();
            match unit.as_str() {
                "%" => LitKind::Percent,
                "ms" | "s" => LitKind::Time,
                "deg" | "rad" | "grad" | "turn" => LitKind::Angle,
                "fr" => LitKind::Flex,
                _ => LitKind::Length,
            }
        }
    }
}

impl Checker<'_, '_, '_> {
    /// A K value component of type `ty`.
    fn value_comp(&self, ty: &Ty) -> Comp {
        let ty = self.resolve(ty);
        if ty.contains_error() {
            return Comp::Value(None);
        }
        match self.deopt(&ty) {
            Ty::Named(d, _) => Comp::Value(Some(self.cx.p.decl(d).name.clone())),
            _ => Comp::Value(Some(String::new())),
        }
    }

    /// Checks a `style` block of the enclosing declaration.
    pub(crate) fn check_style_block(&mut self, block: &SyntaxNode) {
        self.push_fn(super::FnCx {
            is_async: false,
            throws: None,
            lambda_throws: None,
            ret: None,
            is_lambda: false,
            loops: 0,
            dos: Vec::new(),
            tries: Vec::new(),
            awaits: 0,
            init_of: None,
            decl_node: None,
        });
        let mut keyframes: HashMap<String, TextRange> = HashMap::new();
        self.css_items(block, &mut keyframes, false);
        self.pop_fn();
    }

    /// Checks the items of a style block, rule, at-rule or keyframe block; `in_keyframe` restricts them to declarations (K3208).
    fn css_items(
        &mut self,
        node: &SyntaxNode,
        keyframes: &mut HashMap<String, TextRange>,
        in_keyframe: bool,
    ) {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::StyleRule => {
                    if in_keyframe {
                        self.err(
                            codes::KEYFRAME_RULE_CONTENT,
                            child.text_range(),
                            "a keyframe holds declarations only",
                        );
                        continue;
                    }
                    self.selectors(&child);
                    self.css_items(&child, keyframes, false);
                }
                SyntaxKind::Declaration => {
                    if let Some(d) = ast::Declaration::cast(child.clone()) {
                        self.declaration(&d);
                    }
                }
                SyntaxKind::AtRule => {
                    if in_keyframe {
                        self.err(
                            codes::KEYFRAME_RULE_CONTENT,
                            child.text_range(),
                            "a keyframe holds declarations only",
                        );
                        continue;
                    }
                    self.at_rule(&child);
                    self.css_items(&child, keyframes, false);
                }
                SyntaxKind::KeyframesRule => {
                    if in_keyframe {
                        self.err(
                            codes::KEYFRAME_RULE_CONTENT,
                            child.text_range(),
                            "a keyframe holds declarations only",
                        );
                        continue;
                    }
                    self.keyframes_rule(&child, keyframes);
                }
                SyntaxKind::KeyframeBlock => {
                    for sel in child
                        .children()
                        .filter(|c| c.kind() == SyntaxKind::KeyframeSelector)
                    {
                        self.keyframe_selector(&sel);
                    }
                    self.css_items(&child, keyframes, true);
                }
                _ => {}
            }
        }
    }

    /// Records the classes a rule's selectors name.
    fn selectors(&mut self, rule: &SyntaxNode) {
        let Some(owner) = self.style_owner() else {
            return;
        };
        let component = self.cx.p.decl_name(owner);
        for sel in rule.children().filter(|c| c.kind() == SyntaxKind::Selector) {
            for class in sel
                .descendants()
                .filter(|c| c.kind() == SyntaxKind::ClassSelector)
            {
                if let Some(name) = class
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind().is_name_like())
                {
                    self.note_res(
                        name.text_range(),
                        Resolution::Class {
                            component: component.clone(),
                            name: name.text().to_owned(),
                        },
                    );
                }
            }
        }
    }

    /// `@media`, `@container`, `@layer` (K3212 otherwise) and their media
    /// features (K3211).
    fn at_rule(&mut self, node: &SyntaxNode) {
        let Some(name) = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind().is_name_like())
        else {
            return;
        };
        if !css::AT_RULES.contains(&name.text()) {
            self.err(
                codes::UNKNOWN_AT_RULE,
                name.text_range(),
                format!("`@{}` is not a supported at-rule", name.text()),
            );
            return;
        }
        // Keywords of feature values are camel case too (K3214).
        for value in node
            .descendants()
            .filter(|c| c.kind() == SyntaxKind::MediaFeature)
            .flat_map(|f| f.children().filter(|c| c.kind() == SyntaxKind::CssValue))
        {
            for t in value
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind().is_name_like())
            {
                self.css_keyword(&t);
            }
        }
        if name.text() != "media" {
            return;
        }
        for feature in node
            .descendants()
            .filter(|c| c.kind() == SyntaxKind::MediaFeature)
        {
            let Some(f) = feature
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind().is_name_like())
            else {
                continue;
            };
            if css::MEDIA_FEATURES.iter().any(|(k, _)| *k == f.text()) {
                continue;
            }
            let range = f.text_range();
            let mut d = Diagnostic::new(
                codes::UNKNOWN_MEDIA_FEATURE,
                self.span(range),
                format!("`{}` is not a media feature", f.text()),
            );
            let kebab = css::MEDIA_FEATURES
                .iter()
                .find(|(_, c)| *c == f.text())
                .map(|(k, _)| *k);
            if let Some(c) =
                kebab.or_else(|| closest(f.text(), css::MEDIA_FEATURES.iter().map(|(k, _)| *k)))
            {
                let fix = Fix::replace(
                    format!("Replace `{}` with `{c}`", f.text()),
                    self.span(range),
                    c,
                );
                d = d.with_fix(if kebab.is_some() {
                    fix
                } else {
                    fix.maybe_incorrect()
                });
            }
            self.report(d);
        }
    }

    /// `@keyframes name { … }`: duplicates (K3213), the name's resolution and its blocks.
    fn keyframes_rule(&mut self, node: &SyntaxNode, keyframes: &mut HashMap<String, TextRange>) {
        let Some(rule) = ast::KeyframesRule::cast(node.clone()) else {
            return;
        };
        if let Some(name) = rule.name() {
            match keyframes.get(name.text()) {
                Some(first) => {
                    let first = *first;
                    self.report(
                        Diagnostic::new(
                            codes::DUPLICATE_KEYFRAMES,
                            self.span(name.text_range()),
                            format!("keyframes `{}` are declared twice", name.text()),
                        )
                        .with_label(self.span(first), "first declared here"),
                    );
                }
                None => {
                    keyframes.insert(name.text().to_owned(), name.text_range());
                }
            }
            if let Some(owner) = self.style_owner() {
                self.note_res(
                    name.text_range(),
                    Resolution::Keyframes {
                        component: self.cx.p.decl_name(owner),
                        name: name.text().to_owned(),
                    },
                );
            }
        }
        self.css_items(node, keyframes, true);
    }

    /// `from`, `to` or a percentage from `0%` to `100%` (K3207).
    fn keyframe_selector(&mut self, sel: &SyntaxNode) {
        let Some(tok) = sel
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| !t.kind().is_trivia())
        else {
            return;
        };
        let text = tok.text();
        let ok = match text {
            "from" | "to" => true,
            _ => text
                .strip_suffix('%')
                .and_then(|n| n.parse::<f64>().ok())
                .is_some_and(|n| (0.0..=100.0).contains(&n)),
        };
        if !ok {
            self.err(
                codes::INVALID_KEYFRAME_SELECTOR,
                sel.text_range(),
                "a keyframe selector is `from`, `to` or a percentage from `0%` to `100%`",
            );
        }
    }

    /// One `property: value;` declaration.
    fn declaration(&mut self, d: &ast::Declaration) {
        let Some(prop_tok) = d.property() else { return };
        let name = prop_tok.text();
        let Some(prop) = css::property_named(name) else {
            self.unknown_property(&prop_tok);
            // The value isn't checked, but its names still resolve.
            self.quiet += 1;
            for v in d.values() {
                self.css_components(v.syntax(), None);
            }
            self.quiet -= 1;
            return;
        };
        self.note_res(
            prop_tok.text_range(),
            Resolution::CssProperty {
                name: prop.name.to_owned(),
            },
        );
        let mut comps: Vec<Comp> = Vec::new();
        let mut ranges: Vec<TextRange> = Vec::new();
        for (i, v) in d.values().enumerate() {
            if i > 0 {
                comps.push(Comp::Comma);
                ranges.push(v.syntax().text_range());
            }
            for (c, r) in self.css_components(v.syntax(), Some(prop)) {
                comps.push(c);
                ranges.push(r);
            }
        }
        if comps.is_empty()
            || matches!(comps.as_slice(), [Comp::Keyword(k)] if css::WIDE_KEYWORDS.contains(&k.as_str()))
        {
            return;
        }
        let grammar = GrammarParser::parse(prop.syntax);
        let mut m = Matcher {
            comps: &comps,
            far: 0,
        };
        let ends = m.ends(&grammar, 0);
        if ends.contains(&comps.len()) {
            return;
        }
        let at = if m.far < comps.len() {
            ranges[m.far]
        } else {
            let first = ranges.first().copied().unwrap_or(prop_tok.text_range());
            let last = ranges.last().copied().unwrap_or(first);
            TextRange::new(first.start(), last.end())
        };
        self.err(
            codes::INVALID_CSS_VALUE,
            at,
            format!("this doesn't fit `{}`: {}", prop.name, prop.syntax),
        );
    }

    /// K3201 with the camel-case fix for a CSS name, or the closest property.
    fn unknown_property(&mut self, tok: &SyntaxToken) {
        let name = tok.text();
        let range = tok.text_range();
        let mut d = Diagnostic::new(
            codes::UNKNOWN_CSS_PROPERTY,
            self.span(range),
            format!("`{name}` is not a CSS property"),
        );
        let camel = css::PROPERTIES
            .iter()
            .find(|p| p.css == name)
            .map(|p| p.name);
        if let Some(c) = camel {
            d = d.with_fix(Fix::replace(
                format!("Replace `{name}` with `{c}`"),
                self.span(range),
                c,
            ));
        } else if let Some(c) = closest(name, css::PROPERTIES.iter().map(|p| p.name)) {
            d = d.with_fix(
                Fix::replace(format!("Replace `{name}` with `{c}`"), self.span(range), c)
                    .maybe_incorrect(),
            );
        }
        self.report(d);
    }

    /// The components of one comma-separated value group.
    fn css_components(
        &mut self,
        value: &SyntaxNode,
        prop: Option<&'static CssProperty>,
    ) -> Vec<(Comp, TextRange)> {
        let mut out = Vec::new();
        for el in value.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Token(t) => match t.kind() {
                    k if k.is_trivia() => {}
                    SyntaxKind::Slash => out.push((Comp::Slash, t.text_range())),
                    SyntaxKind::Comma => out.push((Comp::Comma, t.text_range())),
                    k if k.is_name_like() => {
                        let word = self.css_keyword(&t);
                        out.push((Comp::Keyword(word), t.text_range()));
                    }
                    _ => out.push((Comp::Any, t.text_range())),
                },
                rowan::NodeOrToken::Node(n) => {
                    let range = n.text_range();
                    let comp = match n.kind() {
                        SyntaxKind::Literal => n
                            .children_with_tokens()
                            .filter_map(rowan::NodeOrToken::into_token)
                            .find(|t| !t.kind().is_trivia())
                            .map_or(Comp::Any, |t| Comp::Literal(literal_kind(&t))),
                        SyntaxKind::StringLit => {
                            if let Some(e) = ast::Expr::cast(n.clone()) {
                                self.infer(&e, None);
                            }
                            Comp::Str
                        }
                        SyntaxKind::Interpolation => self.css_interpolation(&n, prop),
                        SyntaxKind::CssFunction => {
                            let fname = n
                                .children_with_tokens()
                                .filter_map(rowan::NodeOrToken::into_token)
                                .find(|t| t.kind().is_name_like())
                                .map(|t| t.text().to_owned())
                                .unwrap_or_default();
                            let mut args = Vec::new();
                            for (i, v) in n
                                .children()
                                .filter(|c| c.kind() == SyntaxKind::CssValue)
                                .enumerate()
                            {
                                if i > 0 {
                                    args.push(Comp::Comma);
                                }
                                args.extend(
                                    self.css_components(&v, prop).into_iter().map(|(c, _)| c),
                                );
                            }
                            Comp::Func(fname, args)
                        }
                        _ => match ast::Expr::cast(n.clone()) {
                            Some(e) => self.css_expr(&e, prop),
                            None => Comp::Any,
                        },
                    };
                    out.push((comp, range));
                }
            }
        }
        out
    }

    /// A keyword of a value in its camel-case spelling: a kebab-case one
    /// (`space-between`) is K3214 with the machine-applicable camel-case fix
    /// and is matched as that spelling, so the grammar doesn't report it
    /// again.
    fn css_keyword(&mut self, t: &SyntaxToken) -> String {
        let text = t.text();
        if !text.contains('-') {
            return text.to_owned();
        }
        let camel = css::camel_case(text);
        let range = t.text_range();
        self.report(
            Diagnostic::new(
                codes::KEBAB_CASE_CSS_KEYWORD,
                self.span(range),
                format!("CSS keywords are camel case in K: `{text}` is written `{camel}`"),
            )
            .with_fix(Fix::replace(
                format!("Replace `{text}` with `{camel}`"),
                self.span(range),
                camel.clone(),
            )),
        );
        camel
    }

    /// The head implicit member of a K value in a style (`.md`,
    /// `.primary.opacity(0.8)`).
    fn implicit_head(e: &ast::Expr) -> Option<SyntaxToken> {
        let mut current = e.clone();
        loop {
            current = match &current {
                ast::Expr::CallExpr(c) => c.callee()?,
                ast::Expr::MemberExpr(m) => m.receiver()?,
                ast::Expr::ImplicitMemberExpr(m) => {
                    return m
                        .syntax()
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .find(|t| t.kind().is_name_like());
                }
                _ => return None,
            };
        }
    }

    /// A K value in a style: an implicit member resolved against the
    /// property's types, a keyframes reference, or another expression.
    fn css_expr(&mut self, e: &ast::Expr, prop: Option<&'static CssProperty>) -> Comp {
        let Some(prop) = prop else {
            self.no_implicit += 1;
            self.infer(e, None);
            self.no_implicit -= 1;
            return Comp::Any;
        };
        let Some(head) = Self::implicit_head(e) else {
            let t = self.infer(e, None);
            return self.value_comp(&t);
        };
        if prop.types.contains(&"Keyframes") {
            self.keyframes_ref(&head);
            return Comp::Keyframes;
        }
        // The first of the property's types that has the member.
        let types: Vec<Ty> = prop
            .types
            .iter()
            .filter_map(|n| self.cx.p.prelude.get(*n).copied())
            .map(Ty::named)
            .collect();
        let chosen = types
            .iter()
            .find(|t| self.has_static(t, head.text()))
            .cloned();
        match chosen {
            Some(t) => {
                let ty = self.infer(e, Some(&t));
                self.value_comp(&ty)
            }
            None => {
                // A token of another category: the value is wrong, not the
                // name (K3202 from the grammar).
                let elsewhere = self
                    .cx
                    .tokens
                    .iter()
                    .any(|(_, ms)| ms.iter().any(|m| self.cx.p.member(*m).name == head.text()));
                if elsewhere || types.is_empty() {
                    self.quiet += 1;
                    let t = self.infer(e, None);
                    self.quiet -= 1;
                    let t = self.resolve(&t);
                    return if t.contains_error() {
                        Comp::Keyword(String::new())
                    } else {
                        self.value_comp(&t)
                    };
                }
                self.infer(e, types.first());
                Comp::Any
            }
        }
    }

    /// Returns `true` if `ty` has a static member or token `name`.
    fn has_static(&self, ty: &Ty, name: &str) -> bool {
        matches!(
            self.cx
                .lookup_member(self.file(), self.owner.decl, ty, name, true),
            MemberFound::Member(..) | MemberFound::Token(_)
        )
    }

    /// `animationName: .fade` names keyframes of the same block (K3205).
    fn keyframes_ref(&mut self, name: &SyntaxToken) {
        let Some(owner) = self.style_owner() else {
            return;
        };
        let info = self.cx.fw.style(self.cx, owner);
        let text = name.text();
        if info.has_keyframes(text) {
            self.cx
                .fw
                .used_keyframes
                .borrow_mut()
                .insert((owner, text.to_owned()));
            self.note_res(
                name.text_range(),
                Resolution::Keyframes {
                    component: self.cx.p.decl_name(owner),
                    name: text.to_owned(),
                },
            );
            return;
        }
        let range = name.text_range();
        let mut d = Diagnostic::new(
            codes::UNKNOWN_KEYFRAMES,
            self.span(range),
            format!("the style block has no keyframes `{text}`"),
        );
        if let Some(c) = closest(text, info.keyframes.iter().map(|(n, _)| n.as_str())) {
            d = d.with_fix(
                Fix::replace(format!("Replace `{text}` with `{c}`"), self.span(range), c)
                    .maybe_incorrect(),
            );
        }
        self.report(d);
    }

    /// `{ expr }` in a value: a reactive value (K3210 otherwise) of one of
    /// the property's types (K3202 from the grammar).
    fn css_interpolation(&mut self, node: &SyntaxNode, prop: Option<&'static CssProperty>) -> Comp {
        let Some(e) = node.children().find_map(ast::Expr::cast) else {
            return Comp::Any;
        };
        let comp = match (&e, prop) {
            (
                ast::Expr::ImplicitMemberExpr(_)
                | ast::Expr::CallExpr(_)
                | ast::Expr::MemberExpr(_),
                Some(_),
            ) if Self::implicit_head(&e).is_some() => self.css_expr(&e, prop),
            _ => {
                let t = self.infer(&e, None);
                let t = self.resolve(&t);
                if t.contains_error() {
                    return Comp::Any;
                }
                self.value_comp(&t)
            }
        };
        if prop.is_none() || self.reads_reactive(e.syntax()) {
            return comp;
        }
        let range = e.syntax().text_range();
        let mut d = Diagnostic::new(
            codes::NON_REACTIVE_INTERPOLATION,
            self.span(range),
            "`{ }` holds reactive values; this one never changes",
        );
        let constant = matches!(e, ast::Expr::Literal(_)) || Self::implicit_head(&e).is_some();
        if constant {
            let text = self.text(range).to_owned();
            d = d.with_fix(Fix::replace(
                "Remove the braces",
                self.span(node.text_range()),
                text,
            ));
        }
        self.report(d);
        comp
    }

    /// Returns `true` if an expression reads a reactive place.
    pub(crate) fn reads_reactive(&self, node: &SyntaxNode) -> bool {
        node.descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_node)
            .chain(std::iter::once(node.clone()))
            .filter_map(ast::Expr::cast)
            .any(|e| match &e {
                ast::Expr::NameExpr(_) | ast::Expr::MemberExpr(_) => {
                    self.is_reactive_place(&e)
                        || matches!(self.targets.get(&e.syntax().text_range()), Some(Target::Member(m)) if {
                            let member = self.cx.p.member(*m);
                            self.cx.p.decl(member.owner).named_kind() == Some(crate::stdlib::DeclarationKind::Service)
                        })
                }
                _ => false,
            })
    }
}
