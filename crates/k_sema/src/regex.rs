//! A validator for the ECMAScript regular-expression syntax of `@Matches`
//! patterns (sema.md §12.10, K7006).
//!
//! `@Matches` patterns run on the server and in the browser, so the
//! compiler accepts exactly what both accept: the ECMAScript pattern grammar
//! (with Annex B's leniency for a `{` that doesn't start a quantifier). The
//! validator only answers whether a pattern is well formed; it never builds
//! an automaton, runs in linear time and doesn't recurse deeper than the
//! pattern's group nesting, which is bounded by [`MAX_DEPTH`].
//!
//! ```text
//! validate("^[A-Z]{3}$") == Ok(())
//! validate("(ab")        == Err("unterminated group")
//! ```

/// The deepest group nesting accepted.
const MAX_DEPTH: usize = 256;

/// Checks `pattern`; returns why it is invalid.
pub(crate) fn validate(pattern: &str) -> Result<(), &'static str> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut p = Parser {
        chars: &chars,
        pos: 0,
        depth: 0,
    };
    p.disjunction()?;
    if p.pos < chars.len() {
        // Only an unmatched `)` stops a top-level disjunction.
        return Err("unmatched `)`");
    }
    Ok(())
}

/// A recursive-descent parser over the pattern's characters.
struct Parser<'c> {
    /// The pattern.
    chars: &'c [char],
    /// The next character's index.
    pos: usize,
    /// The current group nesting.
    depth: usize,
}

impl Parser<'_> {
    /// The next character, if any.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Consumes `c` if it is next.
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// `a|b|…`.
    fn disjunction(&mut self) -> Result<(), &'static str> {
        self.alternative()?;
        while self.eat('|') {
            self.alternative()?;
        }
        Ok(())
    }

    /// A sequence of terms up to `|` or `)`.
    fn alternative(&mut self) -> Result<(), &'static str> {
        while let Some(c) = self.peek() {
            match c {
                '|' | ')' => break,
                '^' | '$' => self.pos += 1,
                '*' | '+' | '?' => return Err("nothing to repeat"),
                '{' if self.quantifier_ahead() => return Err("nothing to repeat"),
                _ => {
                    let assertion = self.atom()?;
                    if !assertion {
                        self.quantifier()?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Parses one atom; returns `true` for a lookaround assertion, which
    /// takes no quantifier.
    fn atom(&mut self) -> Result<bool, &'static str> {
        let Some(c) = self.peek() else {
            return Ok(false);
        };
        self.pos += 1;
        match c {
            '\\' => {
                if self.peek().is_none() {
                    return Err("`\\` at the end of the pattern");
                }
                let e = self.chars[self.pos];
                self.pos += 1;
                Ok(e == 'b' || e == 'B')
            }
            '[' => self.class().map(|()| false),
            '(' => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err("groups nested too deeply");
                }
                let mut lookaround = false;
                if self.eat('?') {
                    match self.peek() {
                        Some(':') => self.pos += 1,
                        Some('=' | '!') => {
                            self.pos += 1;
                            lookaround = true;
                        }
                        Some('<') => {
                            self.pos += 1;
                            if self.eat('=') || self.eat('!') {
                                lookaround = true;
                            } else {
                                let start = self.pos;
                                while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                                    self.pos += 1;
                                }
                                if self.pos == start || !self.eat('>') {
                                    return Err("invalid group name");
                                }
                            }
                        }
                        _ => return Err("invalid group"),
                    }
                }
                self.disjunction()?;
                if !self.eat(')') {
                    return Err("unterminated group");
                }
                self.depth -= 1;
                Ok(lookaround)
            }
            _ => Ok(false),
        }
    }

    /// A character class after `[`, up to and including `]`.
    fn class(&mut self) -> Result<(), &'static str> {
        self.eat('^');
        let mut prev: Option<char> = None;
        loop {
            let Some(c) = self.peek() else {
                return Err("unterminated character class");
            };
            self.pos += 1;
            match c {
                ']' => return Ok(()),
                '\\' => {
                    if self.peek().is_none() {
                        return Err("unterminated character class");
                    }
                    let e = self.chars[self.pos];
                    self.pos += 1;
                    prev = (!matches!(e, 'd' | 'D' | 'w' | 'W' | 's' | 'S')).then_some(e);
                }
                '-' if prev.is_some() && self.peek().is_some_and(|n| n != ']') => {
                    let lo = prev.unwrap_or('\0');
                    let Some(mut hi) = self.peek() else {
                        return Err("unterminated character class");
                    };
                    self.pos += 1;
                    if hi == '\\' {
                        let Some(e) = self.peek() else {
                            return Err("unterminated character class");
                        };
                        self.pos += 1;
                        hi = e;
                    }
                    if hi < lo {
                        return Err("character class range out of order");
                    }
                    prev = None;
                }
                other => prev = Some(other),
            }
        }
    }

    /// Returns `true` if a `{n}`, `{n,}` or `{n,m}` quantifier starts here.
    fn quantifier_ahead(&self) -> bool {
        self.brace_quantifier(self.pos).is_some()
    }

    /// Parses `{n}`, `{n,}` or `{n,m}` at `at`: the bounds and the end.
    fn brace_quantifier(&self, at: usize) -> Option<(u64, Option<u64>, usize)> {
        let mut i = at;
        if self.chars.get(i) != Some(&'{') {
            return None;
        }
        i += 1;
        let number = |i: &mut usize| -> Option<u64> {
            let start = *i;
            while self.chars.get(*i).is_some_and(char::is_ascii_digit) {
                *i += 1;
            }
            if *i == start {
                return None;
            }
            self.chars[start..*i]
                .iter()
                .collect::<String>()
                .parse()
                .ok()
                .or(Some(u64::MAX))
        };
        let lo = number(&mut i)?;
        let hi = if self.chars.get(i) == Some(&',') {
            i += 1;
            if self.chars.get(i) == Some(&'}') {
                None
            } else {
                Some(number(&mut i)?)
            }
        } else {
            Some(lo)
        };
        if self.chars.get(i) != Some(&'}') {
            return None;
        }
        Some((lo, hi, i + 1))
    }

    /// An optional quantifier after an atom; a second one is "nothing to repeat".
    fn quantifier(&mut self) -> Result<(), &'static str> {
        match self.peek() {
            Some('*' | '+' | '?') => {
                self.pos += 1;
            }
            Some('{') => {
                let Some((lo, hi, end)) = self.brace_quantifier(self.pos) else {
                    // Annex B: a `{` that starts no quantifier is a literal.
                    return Ok(());
                };
                if hi.is_some_and(|h| h < lo) {
                    return Err("numbers out of order in a `{}` quantifier");
                }
                self.pos = end;
            }
            _ => return Ok(()),
        }
        // A lazy `?`; another quantifier is "nothing to repeat".
        self.eat('?');
        match self.peek() {
            Some('*' | '+' | '?') => Err("nothing to repeat"),
            Some('{') if self.quantifier_ahead() => Err("nothing to repeat"),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn valid_patterns_pass() {
        for p in [
            "^ok$",
            "^[A-Z]{3}$",
            "^[0-9]+$",
            "a|b|",
            "(?:ab)+?",
            "(?<year>\\d{4})-(?=x)",
            "[\\]a-]",
            "a{",
            "a{1,}",
            "\\bword\\b",
            "",
        ] {
            assert_eq!(validate(p), Ok(()), "{p}");
        }
    }

    #[test]
    fn invalid_patterns_fail() {
        for p in [
            "(ab", "*a", "a{{2,1}", "[a-z", "ab)", "a**", "\\", "[z-a]", "a{3,1}", "(?x)",
        ] {
            assert!(validate(p).is_err(), "{p}");
        }
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let deep = "(".repeat(10_000) + &")".repeat(10_000);
        assert!(validate(&deep).is_err());
    }
}
