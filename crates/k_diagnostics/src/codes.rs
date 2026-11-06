//! Diagnostic codes and the code registry.
//!
//! Every diagnostic carries a stable [`Code`], rendered as `K` followed by
//! four digits (`K0002`). Codes are never reused: once a code has shipped,
//! it keeps its meaning forever, so documentation, quick-fix tooling and
//! agents can rely on it.
//!
//! # Code ranges
//!
//! | Range | Stage |
//! |---|---|
//! | `K0001`–`K0099` | Lexer |
//! | `K0100`–`K0499` | Parser |
//! | `K1000`–`K1999` | Modules, imports, name resolution, visibility |
//! | `K2000`–`K2999` | Types, expressions, control flow, errors |
//! | `K3000`–`K3999` | Components, views, styles, themes, reactivity |
//! | `K4000`–`K4999` | Services, dependency injection, layering, server/client boundary |
//! | `K5000`–`K5499` | Pages and routing |
//! | `K5500`–`K5999` | Protocols and RPC |
//! | `K6000`–`K6999` | Persistence |
//! | `K7000`–`K7499` | Forms and validation |
//! | `K7500`–`K7999` | Tests |
//! | `K8000`–`K8999` | Lockfiles and packages |
//! | `K9000`–`K9999` | Driver, I/O and internal errors |
//!
//! The registry below is the source of truth; `docs/compiler/diagnostics.md`
//! mirrors it with the exact primary-span convention of each code.

use std::fmt;

use crate::diagnostic::Severity;

/// A stable diagnostic code such as `K0002`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(u16);

impl Code {
    /// Creates a code from its number. `Code::new(2)` is `K0002`.
    #[must_use]
    pub const fn new(number: u16) -> Code {
        Code(number)
    }

    /// Returns the number of this code.
    #[must_use]
    pub const fn number(self) -> u16 {
        self.0
    }

    /// Parses a code from its rendered form, for example `"K0002"`.
    ///
    /// Returns `None` unless the input is `K` followed by exactly four ASCII
    /// digits.
    #[must_use]
    pub fn parse(text: &str) -> Option<Code> {
        let digits = text.strip_prefix('K')?;
        if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok().map(Code)
    }

    /// Returns the registry entry for this code, if it is registered.
    #[must_use]
    pub fn info(self) -> Option<&'static CodeInfo> {
        lookup(self)
    }

    /// Returns the severity registered for this code, or
    /// [`Severity::Error`] if the code is not registered.
    #[must_use]
    pub fn default_severity(self) -> Severity {
        self.info().map_or(Severity::Error, |info| info.severity)
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "K{:04}", self.0)
    }
}

impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// The registry entry describing one diagnostic code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CodeInfo {
    /// The code itself.
    pub code: Code,
    /// A stable kebab-case name, for example `unterminated-string`.
    pub name: &'static str,
    /// The severity diagnostics with this code have by default.
    pub severity: Severity,
    /// A one-sentence summary used by `k explain` and the documentation.
    pub summary: &'static str,
}

/// Defines code constants and the registry from one table, so the two can
/// never drift apart.
macro_rules! codes {
    ($( $(#[$doc:meta])* $const:ident = $num:literal, $name:literal, $sev:ident, $summary:literal; )*) => {
        $(
            $(#[$doc])*
            pub const $const: Code = Code($num);
        )*

        /// Every registered code, sorted by number.
        pub const REGISTRY: &[CodeInfo] = &[
            $( CodeInfo { code: Code($num), name: $name, severity: Severity::$sev, summary: $summary }, )*
        ];
    };
}

codes! {
    // ---------------------------------------------------------------- lexer
    /// A character, or run of characters, that cannot start any token.
    UNEXPECTED_CHARACTER = 1, "unexpected-character", Error,
        "A character that cannot start any token, such as `$` or a non-ASCII letter outside a string.";
    /// A string literal that is not closed before the end of its line.
    UNTERMINATED_STRING = 2, "unterminated-string", Error,
        "A string literal is not closed with `\"` before the end of its line or file.";
    /// A `/*` comment without a matching `*/`.
    UNTERMINATED_BLOCK_COMMENT = 3, "unterminated-block-comment", Error,
        "A block comment is not closed with `*/` before the end of the file.";
    /// An unknown backslash escape in a string literal.
    INVALID_ESCAPE = 4, "invalid-escape", Error,
        "A backslash escape in a string literal is not one of `\\n`, `\\r`, `\\t`, `\\0`, `\\\\`, `\\\"` or `\\u{...}`.";
    /// A `#` literal that is not 3, 4, 6 or 8 hexadecimal digits.
    INVALID_COLOR_LITERAL = 5, "invalid-color-literal", Error,
        "A color literal must be `#` followed by exactly 3, 4, 6 or 8 hexadecimal digits.";
    /// A malformed number literal.
    INVALID_NUMBER_LITERAL = 6, "invalid-number-literal", Error,
        "A number literal is malformed, for example `0x` with no digits, a trailing `_`, or `1e` with no exponent.";
    /// `\{` used in a string; K escapes braces by doubling them.
    BRACE_ESCAPE = 7, "brace-escape", Error,
        "`\\{` is not an escape in K strings; write `{{` for a literal `{`.";
    /// A malformed `\u{...}` escape.
    INVALID_UNICODE_ESCAPE = 8, "invalid-unicode-escape", Error,
        "A `\\u{...}` escape must contain 1 to 6 hexadecimal digits naming a Unicode scalar value.";

    // --------------------------------------------------------------- parser
    /// The generic "expected X, found Y" syntax error.
    EXPECTED_TOKEN = 100, "expected-token", Error,
        "The parser expected a specific token and found something else.";
    /// An opening delimiter without its closing partner.
    UNCLOSED_DELIMITER = 101, "unclosed-delimiter", Error,
        "An opening `(`, `[` or `{` has no matching closing delimiter.";
    /// Two statements, members or items on the same line.
    STATEMENTS_ON_ONE_LINE = 102, "statements-on-one-line", Error,
        "Statements, members and declarations must be separated by a newline.";
    /// A `;` in K code (outside style blocks).
    SEMICOLON = 103, "semicolon", Error,
        "K code does not use semicolons; statements end at the end of the line.";
    /// `&&` instead of `and`.
    SYMBOLIC_AND = 104, "symbolic-and", Error,
        "K writes logical conjunction as `and`, not `&&`.";
    /// `||` instead of `or`.
    SYMBOLIC_OR = 105, "symbolic-or", Error,
        "K writes logical disjunction as `or`, not `||`.";
    /// `!` instead of `not`.
    SYMBOLIC_NOT = 106, "symbolic-not", Error,
        "K writes logical negation as `not`, not `!`.";
    /// A C-style ternary `a ? b : c`.
    TERNARY = 107, "ternary", Error,
        "K has no ternary operator; use an `if` expression.";
    /// A reserved word used where a name is expected.
    RESERVED_WORD = 108, "reserved-word", Error,
        "A reserved keyword (`as`, `is`) cannot be used as a name.";
    /// `when` outside an argument or list element.
    MISPLACED_WHEN = 109, "misplaced-when", Error,
        "`x when condition` is only allowed as a call argument or a list element.";
    /// `bind` outside a call argument.
    MISPLACED_BIND = 110, "misplaced-bind", Error,
        "`bind` is only allowed directly in a call argument.";
    /// An `import` after the first declaration.
    IMPORT_AFTER_DECLARATION = 111, "import-after-declaration", Error,
        "All imports must come before the first declaration in a file.";
    /// A number with a suffix that is not a known unit.
    UNKNOWN_UNIT = 112, "unknown-unit", Error,
        "A number literal has a suffix that is not a known unit.";
    /// A chain of non-associative operators such as `a < b < c`.
    NON_ASSOCIATIVE_CHAIN = 113, "non-associative-chain", Error,
        "Comparison and range operators cannot be chained; add parentheses or split the expression.";
    /// `try` or `await` to the right of a binary operator.
    MISPLACED_TRY = 114, "misplaced-try", Error,
        "`try` and `await` must start the expression they cover; they cannot appear to the right of a binary operator.";
    /// A bare identifier used as a `match` or `catch` pattern.
    BARE_IDENTIFIER_PATTERN = 115, "bare-identifier-pattern", Error,
        "A bare name is not a pattern; write `let name` to bind or `.name` for an enum case.";
    /// An element (type) selector in a style block.
    TYPE_SELECTOR = 116, "type-selector", Error,
        "Style blocks cannot select elements by name; give the element a class and select that.";
    /// A missing `;` between style declarations.
    MISSING_SEMICOLON = 117, "missing-semicolon", Error,
        "Each style declaration except the last in a block must end with `;`.";
    /// `!important` in a style block.
    IMPORTANT_NOT_SUPPORTED = 118, "important-not-supported", Error,
        "`!important` is not supported; scoped styles make it unnecessary.";
    /// `..` used as a range operator.
    INVALID_RANGE_OPERATOR = 119, "invalid-range-operator", Error,
        "`..` is not an operator; use `..<` for a half-open range or `...` for a closed range.";
    /// `T??` nested optional type.
    NESTED_OPTIONAL = 120, "nested-optional", Error,
        "Optional types cannot be nested: `T??` is not allowed.";
    /// An annotation written after a modifier.
    ANNOTATION_ORDER = 121, "annotation-order", Error,
        "Annotations must come before `public`, `private` and `static`.";
    /// An expression was expected.
    EXPECTED_EXPRESSION = 122, "expected-expression", Error,
        "An expression was expected here.";
    /// A type was expected.
    EXPECTED_TYPE = 123, "expected-type", Error,
        "A type was expected here.";
    /// A pattern was expected.
    EXPECTED_PATTERN = 124, "expected-pattern", Error,
        "A pattern was expected here.";
    /// A top-level declaration was expected.
    EXPECTED_DECLARATION = 125, "expected-declaration", Error,
        "A declaration (such as `component`, `struct` or `func`) was expected at the top level of the file.";
    /// A member was expected inside a declaration body.
    EXPECTED_MEMBER = 126, "expected-member", Error,
        "A member (a field, function, block or case) was expected in this declaration body.";
    /// The same modifier written twice.
    DUPLICATE_MODIFIER = 127, "duplicate-modifier", Error,
        "A modifier such as `public` or `static` is written more than once, or conflicting visibilities are combined.";
    /// `{}` inside a string or style value.
    EMPTY_INTERPOLATION = 128, "empty-interpolation", Error,
        "An interpolation `{}` must contain an expression; write `{{` for a literal `{`.";
    /// `throws` without an error type.
    THROWS_WITHOUT_TYPE = 129, "throws-without-type", Error,
        "`throws` must name the error type that is thrown, for example `throws CartError`.";
    /// Several cases declared on one `case` line in an enum.
    MULTIPLE_ENUM_CASES = 130, "multiple-enum-cases", Error,
        "Each enum case is declared with its own `case` keyword.";
    /// A top-level `var`.
    TOP_LEVEL_VAR = 131, "top-level-var", Error,
        "Top-level variables must be `let`; shared mutable state belongs in a service.";
    /// An `#id` selector in a style block.
    ID_SELECTOR = 132, "id-selector", Error,
        "Style blocks cannot select by id; select a class instead.";
    /// A declaration directly at the top level of a style block.
    DECLARATION_OUTSIDE_RULE = 133, "declaration-outside-rule", Error,
        "A style declaration must be inside a rule such as `.name { ... }`.";
    /// Nesting deeper than the parser's limit.
    NESTING_TOO_DEEP = 134, "nesting-too-deep", Error,
        "Blocks, lists, expressions or style rules are nested deeper than the parser supports; the excess is skipped.";

    // --------------------------------------------------------------- driver
    /// A source file that is not valid UTF-8.
    INVALID_UTF8 = 9001, "invalid-utf8", Error,
        "A source file is not valid UTF-8.";
    /// A source file that could not be read.
    IO_ERROR = 9002, "io-error", Error,
        "A file or directory could not be read.";
}

/// Looks up the registry entry for `code`.
#[must_use]
pub fn lookup(code: Code) -> Option<&'static CodeInfo> {
    REGISTRY
        .binary_search_by_key(&code, |info| info.code)
        .ok()
        .and_then(|index| REGISTRY.get(index))
}

/// Looks up a registry entry by its kebab-case name.
#[must_use]
pub fn lookup_name(name: &str) -> Option<&'static CodeInfo> {
    REGISTRY.iter().find(|info| info.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_is_sorted_and_unique() {
        for pair in REGISTRY.windows(2) {
            assert!(
                pair[0].code < pair[1].code,
                "{} then {}",
                pair[0].code,
                pair[1].code
            );
        }
        let names: HashSet<_> = REGISTRY.iter().map(|info| info.name).collect();
        assert_eq!(names.len(), REGISTRY.len(), "duplicate code names");
    }

    #[test]
    fn names_are_kebab_case_and_summaries_end_with_period() {
        for info in REGISTRY {
            assert!(
                info.name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{}",
                info.name
            );
            assert!(info.summary.ends_with('.'), "{} summary", info.code);
        }
    }

    #[test]
    fn codes_render_with_four_digits_and_parse_back() {
        assert_eq!(Code::new(2).to_string(), "K0002");
        assert_eq!(Code::new(9001).to_string(), "K9001");
        for info in REGISTRY {
            assert_eq!(Code::parse(&info.code.to_string()), Some(info.code));
        }
    }

    #[test]
    fn parse_rejects_malformed_codes() {
        for bad in [
            "", "K", "K1", "K12345", "k0001", "E0001", "K00a1", "K+001", " K0001",
        ] {
            assert_eq!(Code::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn lookup_finds_registered_codes_only() {
        assert_eq!(
            lookup(UNTERMINATED_STRING).map(|i| i.name),
            Some("unterminated-string")
        );
        assert_eq!(lookup(Code::new(4242)), None);
        assert_eq!(lookup_name("semicolon").map(|i| i.code), Some(SEMICOLON));
        assert_eq!(Code::new(4242).default_severity(), Severity::Error);
    }

    #[test]
    fn codes_fall_in_documented_ranges() {
        for info in REGISTRY {
            let n = info.code.number();
            let ok = matches!(n, 1..=99 | 100..=499 | 1000..=9999);
            assert!(ok, "{} is outside every documented range", info.code);
        }
    }
}
