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
//! | `K1000`–`K1999` | Modules, imports, name resolution, visibility, doc comments |
//! | `K2000`–`K2999` | Types, expressions, effects, control flow, errors |
//! | `K3000`–`K3999` | Components, views, styles, themes, reactivity |
//! | `K4000`–`K4999` | Services, dependency injection, layering, server/client boundary |
//! | `K5000`–`K5499` | Pages and routing |
//! | `K5500`–`K5999` | Protocols and RPC |
//! | `K6000`–`K6999` | Persistence |
//! | `K7000`–`K7499` | Forms and validation |
//! | `K7500`–`K7999` | Tests |
//! | `K8000`–`K8999` | Lockfiles, manifest and packages |
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

    // ---------------------------------------------------------------- project layout and files
    /// A `.k` file is outside the places the project layout allows: `App.k`, `src/<module>/` and `migrations/`.
    STRAY_SOURCE_FILE = 1001, "stray-source-file", Error,
        "A `.k` file is outside the places the project layout allows: `App.k`, `src/<module>/` and `migrations/`.";
    /// A file's public declaration must be named like the file.
    PUBLIC_NAME_MISMATCH = 1002, "public-name-mismatch", Error,
        "A file's public declaration must be named like the file.";
    /// A file has at most one public declaration.
    MULTIPLE_PUBLIC_DECLARATIONS = 1003, "multiple-public-declarations", Error,
        "A file has at most one public declaration.";
    /// Two declarations in one module have the same name.
    DUPLICATE_DECLARATION = 1004, "duplicate-declaration", Error,
        "Two declarations in one module have the same name.";
    /// A declaration kind appears where the layout doesn't allow it, such as `app` outside `App.k` or `migration` outside `migrations/`.
    MISPLACED_DECLARATION = 1005, "misplaced-declaration", Error,
        "A declaration kind appears where the layout doesn't allow it, such as `app` outside `App.k` or `migration` outside `migrations/`.";

    // ---------------------------------------------------------------- imports and the module graph
    /// An import names a declaration that doesn't exist.
    UNRESOLVED_IMPORT = 1101, "unresolved-import", Error,
        "An import names a declaration that doesn't exist.";
    /// An import names a declaration that isn't visible outside its module.
    IMPORT_NOT_VISIBLE = 1102, "import-not-visible", Error,
        "An import names a declaration that isn't visible outside its module.";
    /// Modules import each other in a cycle.
    IMPORT_CYCLE = 1103, "import-cycle", Error,
        "Modules import each other in a cycle.";
    /// Only test files and test-only modules may import from a `testing` module or from `k.test`.
    TEST_ONLY_IMPORT = 1104, "test-only-import", Error,
        "Only test files and test-only modules may import from a `testing` module or from `k.test`.";
    /// An import is never used.
    UNUSED_IMPORT = 1105, "unused-import", Warning,
        "An import is never used.";
    /// Two imports bring the same name into one file.
    DUPLICATE_IMPORT = 1106, "duplicate-import", Error,
        "Two imports bring the same name into one file.";

    // ---------------------------------------------------------------- name resolution
    /// A name doesn't refer to any local, parameter, member or declaration in scope.
    UNRESOLVED_NAME = 1201, "unresolved-name", Error,
        "A name doesn't refer to any local, parameter, member or declaration in scope.";
    /// A type name doesn't refer to any visible type.
    UNRESOLVED_TYPE = 1202, "unresolved-type", Error,
        "A type name doesn't refer to any visible type.";
    /// A type has no member with this name.
    UNKNOWN_MEMBER = 1203, "unknown-member", Error,
        "A type has no member with this name.";

    // ---------------------------------------------------------------- visibility
    /// A declaration or member is used where its visibility doesn't reach.
    NOT_VISIBLE = 1301, "not-visible", Error,
        "A declaration or member is used where its visibility doesn't reach.";
    /// Members are visible wherever their type is, so `public` on a member is redundant.
    REDUNDANT_PUBLIC_MEMBER = 1302, "redundant-public-member", Warning,
        "Members are visible wherever their type is, so `public` on a member is redundant.";
    /// `@Inject` fields are always private to their declaration.
    PUBLIC_INJECT = 1303, "public-inject", Error,
        "`@Inject` fields are always private to their declaration.";
    /// An extension is `public` or visible in its module; it can't be `private`.
    PRIVATE_EXTEND = 1304, "private-extend", Error,
        "An extension is `public` or visible in its module; it can't be `private`.";
    /// Two visible extensions add a member with the same name to one type.
    CONFLICTING_EXTENSION_MEMBER = 1305, "conflicting-extension-member", Error,
        "Two visible extensions add a member with the same name to one type.";

    // ---------------------------------------------------------------- stability
    /// A `@Deprecated` declaration is used.
    DEPRECATED = 1401, "deprecated", Warning,
        "A declaration marked `@Deprecated` is used; it will be removed in a later minor, and its replacement should be used instead.";

    // ---------------------------------------------------------------- doc comments
    /// An intra-doc link names nothing that resolves.
    UNRESOLVED_DOC_LINK = 1901, "unresolved-doc-link", Warning,
        "An intra-doc link names nothing that resolves.";

    // ---------------------------------------------------------------- types and expressions
    /// An expression's type doesn't match the type expected here.
    TYPE_MISMATCH = 2001, "type-mismatch", Error,
        "An expression's type doesn't match the type expected here.";
    /// The type of an expression can't be inferred; write it out.
    CANNOT_INFER = 2002, "cannot-infer", Error,
        "The type of an expression can't be inferred; write it out.";
    /// A call doesn't pass a required argument.
    MISSING_ARGUMENT = 2003, "missing-argument", Error,
        "A call doesn't pass a required argument.";
    /// A call passes an argument label the callee doesn't declare.
    UNKNOWN_ARGUMENT_LABEL = 2004, "unknown-argument-label", Error,
        "A call passes an argument label the callee doesn't declare.";
    /// The expression called is not a function, initializer or element.
    NOT_CALLABLE = 2005, "not-callable", Error,
        "The expression called is not a function, initializer or element.";
    /// An implicit member `.name` doesn't exist on the expected type.
    UNKNOWN_IMPLICIT_MEMBER = 2006, "unknown-implicit-member", Error,
        "An implicit member `.name` doesn't exist on the expected type.";
    /// An implicit member `.name` needs an expected type to resolve against.
    IMPLICIT_MEMBER_WITHOUT_TYPE = 2007, "implicit-member-without-type", Error,
        "An implicit member `.name` needs an expected type to resolve against.";
    /// A memberwise initializer omits a field that has no default value.
    MISSING_MEMBERWISE_FIELD = 2008, "missing-memberwise-field", Error,
        "A memberwise initializer omits a field that has no default value.";
    /// A generic argument doesn't implement the interface its `where` clause requires.
    UNSATISFIED_CONSTRAINT = 2009, "unsatisfied-constraint", Error,
        "A generic argument doesn't implement the interface its `where` clause requires.";
    /// A type that implements an interface doesn't provide a required member.
    MISSING_INTERFACE_MEMBER = 2010, "missing-interface-member", Error,
        "A type that implements an interface doesn't provide a required member.";
    /// A `let` binding or field can't be assigned or mutated.
    ASSIGNMENT_TO_LET = 2011, "assignment-to-let", Error,
        "A `let` binding or field can't be assigned or mutated.";
    /// Comparing a `Bool` with `true` or `false` is redundant.
    BOOL_LITERAL_COMPARISON = 2012, "bool-literal-comparison", Warning,
        "Comparing a `Bool` with `true` or `false` is redundant.";
    /// An operator is not defined for its operand types.
    INVALID_OPERANDS = 2013, "invalid-operands", Error,
        "An operator is not defined for its operand types, such as `==` on a type that isn't `Equatable`.";
    /// `implements` names a type that is not an interface; K has no inheritance.
    NOT_AN_INTERFACE = 2014, "not-an-interface", Error,
        "`implements` names a struct, class, enum or built-in type; only interfaces can be implemented.";
    /// A member meant to satisfy an interface requirement has another signature.
    INTERFACE_MEMBER_MISMATCH = 2015, "interface-member-mismatch", Error,
        "A member with a requirement's name has a different type or labels than the requirement.";
    /// An extension declares a stored field.
    STORED_FIELD_IN_EXTENSION = 2016, "stored-field-in-extension", Error,
        "Extensions can add methods, computed properties and static members, but not stored fields.";
    /// A member of an optional value is used without unwrapping it.
    UNWRAP_REQUIRED = 2017, "unwrap-required", Error,
        "A member of an optional value is used without `?.`, `if let`, `guard let` or `??`.";
    /// `?.` or `??` is applied to a value that is not optional.
    NEEDLESS_OPTIONAL_OPERATOR = 2018, "needless-optional-operator", Warning,
        "`?.` or `??` is applied to a value that can't be `nil`.";
    /// A declaration has two non-function members with the same name.
    DUPLICATE_MEMBER = 2019, "duplicate-member", Error,
        "Two fields, computed properties or cases of one declaration share a name (functions may be overloaded).";
    /// Explicit type arguments don't match the generic parameters they are given to.
    TYPE_ARGUMENT_COUNT = 2020, "type-argument-count", Error,
        "Explicit type arguments (`decode<User>(json)`, `List<Int>`) must give one type per generic parameter; a name that isn't generic takes none.";

    // ---------------------------------------------------------------- units and number extensions
    /// Arithmetic mixes values of different dimensions, such as a length and a duration.
    MIXED_UNIT_DIMENSIONS = 2101, "mixed-unit-dimensions", Error,
        "Arithmetic mixes values of different dimensions, such as a length and a duration.";

    // ---------------------------------------------------------------- effects: async and throws
    /// A call that can throw must be marked with `try`.
    MISSING_TRY = 2201, "missing-try", Error,
        "A call that can throw must be marked with `try`.";
    /// `try` marks an expression that can't throw.
    NEEDLESS_TRY = 2202, "needless-try", Warning,
        "`try` marks an expression that can't throw.";
    /// An error thrown here is neither caught nor declared by the enclosing function's `throws`.
    UNHANDLED_ERROR = 2203, "unhandled-error", Error,
        "An error thrown here is neither caught nor declared by the enclosing function's `throws`.";
    /// An async call or a query must be awaited.
    MISSING_AWAIT = 2204, "missing-await", Error,
        "An async call or a query must be awaited.";
    /// `await` is used outside an async function or handler.
    AWAIT_OUTSIDE_ASYNC = 2205, "await-outside-async", Error,
        "`await` is used outside an async function or handler.";
    /// The `catch` clauses of a `do` don't handle every case of the thrown error.
    CATCH_NOT_EXHAUSTIVE = 2206, "catch-not-exhaustive", Error,
        "The `catch` clauses of a `do` don't handle every case of the thrown error.";
    /// The clauses before a `catch unknown` don't name every case of the thrown error: a case was added since they were written.
    UNKNOWN_CATCH_NOT_EXHAUSTIVE = 2207, "unknown-catch-not-exhaustive", Warning,
        "The clauses before a `catch unknown` don't name every case of the thrown error: a case was added since they were written, and `catch unknown` handles it for now.";

    // ---------------------------------------------------------------- control flow
    /// A `match` doesn't handle every value of its subject.
    MATCH_NOT_EXHAUSTIVE = 2301, "match-not-exhaustive", Error,
        "A `match` doesn't handle every value of its subject.";
    /// A `match` arm, `catch` clause or `default` can never run.
    UNREACHABLE_ARM = 2302, "unreachable-arm", Error,
        "A `match` arm, `catch` clause or `default` can never run.";
    /// A top-level `case _` arm is written `default`.
    WILDCARD_CASE_ARM = 2303, "wildcard-case-arm", Error,
        "A top-level `case _` arm is written `default`.";
    /// A catch-all `case let x` or `catch let x` whose binding is unused is written `default` or `catch _`.
    UNUSED_CATCH_ALL_BINDING = 2304, "unused-catch-all-binding", Error,
        "A catch-all `case let x` or `catch let x` whose binding is unused is written `default` or `catch _`.";
    /// The `else` block of a `guard` must leave the scope with `return`, `throw`, `break` or `continue`.
    GUARD_DOES_NOT_EXIT = 2305, "guard-does-not-exit", Error,
        "The `else` block of a `guard` must leave the scope with `return`, `throw`, `break` or `continue`.";
    /// A binding is read before it is definitely initialized.
    USE_BEFORE_INITIALIZATION = 2306, "use-before-initialization", Error,
        "A binding is read before it is definitely initialized.";
    /// A function with a return type can end without returning a value.
    MISSING_RETURN = 2307, "missing-return", Error,
        "A function with a return type can end without returning a value.";
    /// A lambda's value is its last expression, so `return` can't be used inside a lambda.
    RETURN_IN_LAMBDA = 2308, "return-in-lambda", Error,
        "A lambda's value is its last expression, so `return` can't be used inside a lambda.";
    /// `break` or `continue` is used outside a loop, or crosses a lambda.
    MISPLACED_JUMP = 2309, "misplaced-jump", Error,
        "`break` or `continue` is used outside a loop, or crosses a lambda.";
    /// A `Stream` is iterated with `for await` inside an async function.
    FOR_OVER_STREAM = 2310, "for-over-stream", Error,
        "A `Stream` is iterated with `for await` inside an async function.";
    /// `for await` iterates only a `Stream`, inside an async function.
    FOR_AWAIT_NOT_STREAM = 2311, "for-await-not-stream", Error,
        "`for await` iterates only a `Stream`, inside an async function.";
    /// A case pattern names an enum the matched value's type doesn't say it is (`Error`, a type parameter).
    CASE_OF_UNKNOWN_ENUM = 2312, "case-of-unknown-enum", Error,
        "A case pattern names an enum the matched value's type doesn't say it is (an `Error`, a type parameter): which enum it is isn't known when the program runs.";

    // ---------------------------------------------------------------- stdlib-specific rules
    /// `HttpStatus.other(code)` can't hold a code that has a named case.
    NAMED_STATUS_AS_OTHER = 2401, "named-status-as-other", Error,
        "`HttpStatus.other(code)` can't hold a code that has a named case.";
    /// A sum over `Money` needs a currency: `sum(in:_:)`, not `sum()`.
    MONEY_SUM_WITHOUT_CURRENCY = 2402, "money-sum-without-currency", Error,
        "A sum over `Money` needs a currency: `sum(in:_:)`, not `sum()`.";
    /// A checked literal's initializer (`Url("…")`, `HttpStatus(code:)`) is given something other than a literal.
    LITERAL_REQUIRED = 2403, "literal-required", Error,
        "A checked literal's initializer, such as `Url(\"…\")` or `HttpStatus(code:)`, takes a literal; text known only at run time goes through `Type.parse`.";
    /// A checked literal is not a valid value of its type.
    INVALID_LITERAL = 2404, "invalid-literal", Error,
        "A checked literal (`Url`, `Uuid`, `EmailAddress`, `TimeOfDay`, `HttpStatus(code:)` or a money amount) is not a valid value of its type.";
    /// Two storage keys of the app have the same name.
    DUPLICATE_STORAGE_KEY = 2405, "duplicate-storage-key", Error,
        "Two storage keys of the app have the same name, so they would read and write each other's data.";
    /// Two drag kinds have the same name but different payload types.
    CONFLICTING_DRAG_KIND = 2406, "conflicting-drag-kind", Error,
        "Two drag kinds with the same name are the same kind, so they must carry the same payload type.";
    /// `Money + Money` or `Money - Money` over two different literal currencies.
    MIXED_CURRENCIES = 2407, "mixed-currencies", Error,
        "`+` and `-` on `Money` need one currency; two literal amounts in different currencies would panic at run time.";
    /// Two tab channels, or two data channel kinds, have the same name but different message types.
    CONFLICTING_CHANNEL = 2408, "conflicting-channel", Error,
        "Two `TabChannel`s, or two `DataChannelKind`s, with the same name are the same channel, so they must carry the same message type.";
    /// Two data channel kinds with different names would share one channel id.
    COLLIDING_DATA_CHANNELS = 2409, "colliding-data-channels", Error,
        "Two `DataChannelKind`s with different names would share one channel id, which both peers derive from the name; rename one.";
    /// Two collections of the app have the same name, or two indexes the same collection and field.
    DUPLICATE_COLLECTION = 2410, "duplicate-collection", Error,
        "Two collections of the app have the same name, or two indexes the same collection and field, so they would share one IndexedDB store or index.";
    /// A collection's key field or an index's field isn't a stored field of the value type with a key type.
    INVALID_COLLECTION_FIELD = 2411, "invalid-collection-field", Error,
        "A collection's `key:` or an index's `field:` must name, as a string literal, a stored field of the value type whose type is the declared key or field type: `String`, `Int`, `Uuid`, `Date` or an enum without payloads.";
    /// A collection index names no collection of its value type.
    UNKNOWN_COLLECTION = 2412, "unknown-collection", Error,
        "A `CollectionIndex` names a collection that no `Collection` of the same value type declares.";

    // ---------------------------------------------------------------- annotations, members and reactivity
    /// An annotated field must be declared `let` or `var` as its annotation requires.
    ANNOTATION_MUTABILITY = 3001, "annotation-mutability", Error,
        "An annotated field must be declared `let` or `var` as its annotation requires.";
    /// An annotation is used on a declaration or member it doesn't apply to.
    ANNOTATION_NOT_ALLOWED = 3002, "annotation-not-allowed", Error,
        "An annotation is used on a declaration or member it doesn't apply to.";
    /// A component, page or layout needs a `view` block (an endpoint page has none).
    MISSING_VIEW = 3003, "missing-view", Error,
        "A component, page or layout needs a `view` block (an endpoint page has none).";
    /// A declaration has more than one `view`, `style`, `metadata`, `loading` or `failed` block.
    DUPLICATE_BLOCK = 3004, "duplicate-block", Error,
        "A declaration has more than one `view`, `style`, `metadata`, `loading` or `failed` block.";
    /// A `@Slot`, `@Stylable` or `@Emits` field's type doesn't fit its annotation (`View`, `Classes`, `Event<T>`).
    INVALID_ANNOTATED_TYPE = 3005, "invalid-annotated-type", Error,
        "A `@Slot`, `@Stylable` or `@Emits` field's type doesn't fit its annotation (`View`, `Classes`, `Event<T>`).";
    /// A `@Derived` value is computed and can't be assigned.
    WRITE_TO_DERIVED = 3010, "write-to-derived", Error,
        "A `@Derived` value is computed and can't be assigned.";
    /// A `@Derived` initializer can't write state.
    STATE_WRITE_IN_DERIVED = 3011, "state-write-in-derived", Error,
        "A `@Derived` initializer can't write state.";
    /// `@Effect` functions and event handlers handle their own errors, so they can't be `throws`.
    THROWING_EFFECT_OR_HANDLER = 3012, "throwing-effect-or-handler", Error,
        "`@Effect` functions and event handlers handle their own errors, so they can't be `throws`.";
    /// A `TimerHandle` or a `Subscription` is discarded in an effect, so the timer or the callbacks outlive the component.
    DISCARDED_TIMER_HANDLE = 3013, "discarded-timer-handle", Warning,
        "The `TimerHandle` returned by `clock.every` or `clock.after`, or a `Subscription`, is discarded in an effect, so the timer or the callbacks outlive the component.";
    /// A component writes to its own `@Prop`, a one-way input; make it a `@Binding`.
    WRITE_TO_PROP = 3014, "write-to-prop", Error,
        "A component writes to its own `@Prop`, a one-way input; make it a `@Binding`.";
    /// `bind` needs a `@State`, `@Binding` or `@Form` place, or a field of one.
    NOT_BINDABLE = 3015, "not-bindable", Error,
        "`bind` needs a `@State`, `@Binding` or `@Form` place, or a field of one.";
    /// A value passed to a `@Binding` prop or a `Binding` argument needs `bind`.
    MISSING_BIND = 3016, "missing-bind", Error,
        "A value passed to a `@Binding` prop or a `Binding` argument needs `bind`.";
    /// A function passed to `.on(…)` is `throws`; handlers handle their own errors.
    THROWING_HANDLER = 3017, "throwing-handler", Error,
        "A function passed to `.on(…)` is `throws`; handlers handle their own errors.";
    /// `bind` is passed where no binding is expected.
    UNEXPECTED_BIND = 3018, "unexpected-bind", Error,
        "`bind` is passed where no binding is expected.";
    /// `@Load` initializers depend on each other in a cycle.
    LOAD_CYCLE = 3020, "load-cycle", Error,
        "`@Load` initializers depend on each other in a cycle.";
    /// A `@Load` value must be `Codable`, because server-rendered data is sent to the browser.
    LOAD_NOT_CODABLE = 3021, "load-not-codable", Error,
        "A `@Load` value must be `Codable`, because server-rendered data is sent to the browser.";
    /// A declaration with `@Load` fields needs a `loading` block.
    MISSING_LOADING_BLOCK = 3022, "missing-loading-block", Error,
        "A declaration with `@Load` fields needs a `loading` block.";
    /// A declaration with `@Load` fields has a generated `reload()`, so it can't declare a member named `reload`.
    RESERVED_RELOAD = 3023, "reserved-reload", Error,
        "A declaration with `@Load` fields has a generated `reload()`, so it can't declare a member named `reload`.";
    /// The errors a `@Load` initializer can throw must be `Codable`, because a server-rendered `failed` is hydrated.
    LOAD_ERROR_NOT_CODABLE = 3024, "load-error-not-codable", Error,
        "The errors a `@Load` initializer can throw must be `Codable`, because a server-rendered `failed` is hydrated.";
    /// `status(_:)` is allowed only in a `metadata` block inside `failed`.
    STATUS_OUTSIDE_FAILED = 3025, "status-outside-failed", Error,
        "`status(_:)` is allowed only in a `metadata` block inside `failed`.";
    /// A `@Derived` field depends on itself, directly or through other `@Derived` fields.
    DERIVED_CYCLE = 3026, "derived-cycle", Error,
        "A `@Derived` field's initializer reads the field itself, directly or through other `@Derived` fields.";
    /// A view page of an app has no `title`, from its own `metadata`, its layout's or the app's.
    PAGE_WITHOUT_TITLE = 3027, "page-without-title", Warning,
        "A view page has no `title`: neither its `metadata`, nor its layout's, nor the app's sets one.";
    /// A literal `description(…)` is longer than 160 characters, which search results cut off.
    LONG_DESCRIPTION = 3028, "long-description", Warning,
        "A literal `description(…)` is longer than 160 characters, which search results cut off.";

    // ---------------------------------------------------------------- views
    /// A lowercase call in a view is not a built-in element; an HTML name gets a hint (`a` is `link`).
    UNKNOWN_ELEMENT = 3101, "unknown-element", Error,
        "A lowercase call in a view is not a built-in element; an HTML name gets a hint (`a` is `link`).";
    /// A built-in element requires a named argument that is missing.
    MISSING_ELEMENT_ARGUMENT = 3102, "missing-element-argument", Error,
        "A built-in element requires a named argument that is missing.";
    /// An element or component doesn't define the chained attribute or prop.
    UNKNOWN_ATTRIBUTE = 3103, "unknown-attribute", Error,
        "An element or component doesn't define the chained attribute or prop.";
    /// A chained attribute or optional prop, `.class()` included, is set twice on one element.
    DUPLICATE_ATTRIBUTE = 3104, "duplicate-attribute", Error,
        "A chained attribute or optional prop, `.class()` included, is set twice on one element.";
    /// `.on(name:)` names an event the element or component doesn't fire.
    UNKNOWN_EVENT = 3105, "unknown-event", Error,
        "`.on(name:)` names an event the element or component doesn't fire.";
    /// A `for` loop in a view needs a `key`.
    MISSING_FOR_KEY = 3106, "missing-for-key", Error,
        "A `for` loop in a view needs a `key`.";
    /// An element's trailing block holds content the element doesn't accept.
    CONTENT_NOT_ALLOWED = 3107, "content-not-allowed", Error,
        "An element's trailing block holds content the element doesn't accept.";
    /// `.on(…)` takes exactly one event; use one call per event.
    MULTIPLE_EVENTS_IN_ON = 3108, "multiple-events-in-on", Error,
        "`.on(…)` takes exactly one event; use one call per event.";
    /// A heading level is an integer literal from 1 to 6.
    INVALID_HEADING_LEVEL = 3109, "invalid-heading-level", Error,
        "A heading level is an integer literal from 1 to 6.";
    /// `.role(…)` names a role a semantic element expresses; use that element.
    REDUNDANT_ROLE = 3110, "redundant-role", Error,
        "`.role(…)` names a role a semantic element expresses; use that element.";
    /// A `key` clause belongs to a `for` in a view.
    KEY_OUTSIDE_VIEW = 3111, "key-outside-view", Error,
        "A `key` clause belongs to a `for` in a view.";
    /// A `video` playing a file or a URL has no `.captions(…)`.
    MISSING_CAPTIONS = 3112, "missing-captions", Warning,
        "A `video` playing a file or a URL has no `.captions(…)`; add a track, or `.captions(.notNeeded)` for a video without speech.";
    /// An `ElementRef` that isn't a `let` field of a component, page or layout initialized with `ElementRef()`, or a `.ref(…)` naming anything else.
    MISPLACED_ELEMENT_REF = 3113, "misplaced-element-ref", Error,
        "An element reference is a `let` field of the component, page or layout whose element it names, initialized with `ElementRef()`, and `.ref(…)` takes such a field of its own declaration.";
    /// An element reference attached twice, or inside a `for` body or a scoped slot.
    ELEMENT_REF_REUSED = 3114, "element-ref-reused", Error,
        "An element reference names one element: `.ref(…)` attaches it once, outside any `for` body and scoped slot.";
    /// An element reference no `.ref(…)` attaches.
    UNATTACHED_ELEMENT_REF = 3115, "unattached-element-ref", Warning,
        "An element reference that no `.ref(…)` attaches, so its operations never do anything.";
    /// `focus`, `blur` or `scrollIntoView` called from code that may run while a view renders.
    ELEMENT_REF_DURING_RENDER = 3116, "element-ref-during-render", Error,
        "An element reference's `focus`, `blur` or `scrollIntoView` is called from a view, a `@Derived`, a `@Load` or another field initializer; call it from a handler, an effect or `init()`.";

    // ---------------------------------------------------------------- styles
    /// A style declaration names a property that doesn't exist.
    UNKNOWN_CSS_PROPERTY = 3201, "unknown-css-property", Error,
        "A style declaration names a property that doesn't exist.";
    /// A property's value doesn't match the property's value grammar or K types.
    INVALID_CSS_VALUE = 3202, "invalid-css-value", Error,
        "A property's value doesn't match the property's value grammar or K types.";
    /// A class reference names a class the component's style block doesn't declare.
    UNKNOWN_CLASS = 3203, "unknown-class", Error,
        "A class reference names a class the component's style block doesn't declare.";
    /// A class in a style block is never referenced.
    UNUSED_CLASS = 3204, "unused-class", Warning,
        "A class in a style block is never referenced.";
    /// `animationName` names keyframes the style block doesn't declare.
    UNKNOWN_KEYFRAMES = 3205, "unknown-keyframes", Error,
        "`animationName` names keyframes the style block doesn't declare.";
    /// Keyframes in a style block are never referenced.
    UNUSED_KEYFRAMES = 3206, "unused-keyframes", Warning,
        "Keyframes in a style block are never referenced.";
    /// A keyframe selector is `from`, `to` or a percentage from `0%` to `100%`.
    INVALID_KEYFRAME_SELECTOR = 3207, "invalid-keyframe-selector", Error,
        "A keyframe selector is `from`, `to` or a percentage from `0%` to `100%`.";
    /// A keyframe block holds declarations only.
    KEYFRAME_RULE_CONTENT = 3208, "keyframe-rule-content", Error,
        "A keyframe block holds declarations only.";
    /// A `@Stylable` prop is never applied to an element with `.class(…)`.
    UNAPPLIED_STYLABLE = 3209, "unapplied-stylable", Error,
        "A `@Stylable` prop is never applied to an element with `.class(…)`.";
    /// `{ }` in a style value holds only reactive values (props and state).
    NON_REACTIVE_INTERPOLATION = 3210, "non-reactive-interpolation", Error,
        "`{ }` in a style value holds only reactive values (props and state).";
    /// A media query names a feature that doesn't exist.
    UNKNOWN_MEDIA_FEATURE = 3211, "unknown-media-feature", Error,
        "A media query names a feature that doesn't exist.";
    /// A style block uses an at-rule other than `@media`, `@container`, `@layer` and `@keyframes`.
    UNKNOWN_AT_RULE = 3212, "unknown-at-rule", Error,
        "A style block uses an at-rule other than `@media`, `@container`, `@layer` and `@keyframes`.";
    /// A style block declares two keyframes with the same name.
    DUPLICATE_KEYFRAMES = 3213, "duplicate-keyframes", Error,
        "A style block declares two keyframes with the same name.";
    /// A CSS keyword in a style value is written in kebab case; K spells keywords in camel case.
    KEBAB_CASE_CSS_KEYWORD = 3214, "kebab-case-css-keyword", Error,
        "A CSS keyword in a style value is written in kebab case; K spells keywords in camel case.";

    // ---------------------------------------------------------------- themes
    /// An override or variant names a token no theme declares.
    UNKNOWN_THEME_TOKEN = 3301, "unknown-theme-token", Error,
        "An override or variant names a token no theme declares.";
    /// A theme re-declares a token a library already declares; override it with `name = value`.
    REDECLARED_THEME_TOKEN = 3302, "redeclared-theme-token", Error,
        "A theme re-declares a token a library already declares; override it with `name = value`.";
    /// A member of the form `name = value` is a theme override and is allowed only in a `theme`.
    OVERRIDE_OUTSIDE_THEME = 3303, "override-outside-theme", Error,
        "A member of the form `name = value` is a theme override and is allowed only in a `theme`.";
    /// A theme token's first word names no token category.
    UNKNOWN_TOKEN_CATEGORY = 3304, "unknown-token-category", Error,
        "A theme token's first word names no token category.";
    /// A theme token's type is not its category's type.
    TOKEN_CATEGORY_MISMATCH = 3305, "token-category-mismatch", Error,
        "A theme token's type is not its category's type.";
    /// `base` means no variant, so it can't name a theme variant.
    VARIANT_NAMED_BASE = 3306, "variant-named-base", Error,
        "`base` means no variant, so it can't name a theme variant.";
    /// A theme declares a token or variant twice, or a variant overrides a token twice.
    DUPLICATE_THEME_MEMBER = 3307, "duplicate-theme-member", Error,
        "A theme declares a token or variant twice, or a variant overrides a token twice.";

    // ---------------------------------------------------------------- services, di, layering and the server/client boundary
    /// An injected interface has no implementation.
    MISSING_IMPLEMENTATION = 4001, "missing-implementation", Error,
        "An injected interface has no implementation.";
    /// An injected interface has several implementations and no `use` rule picks one.
    AMBIGUOUS_IMPLEMENTATION = 4002, "ambiguous-implementation", Error,
        "An injected interface has several implementations and no `use` rule picks one.";
    /// Injected services depend on each other in a cycle.
    DEPENDENCY_CYCLE = 4003, "dependency-cycle", Error,
        "Injected services depend on each other in a cycle.";
    /// A service injects a service with a shorter scope, or a `.server` service or job reaches request state.
    SCOPE_VIOLATION = 4004, "scope-violation", Error,
        "A service injects a service with a shorter scope, or a `.server` service or job reaches request state.";
    /// A declaration injects something its kind may not inject (components never inject protocols).
    LAYERING_VIOLATION = 4005, "layering-violation", Error,
        "A declaration injects something its kind may not inject (components never inject protocols).";
    /// Only a service's own methods can write its fields.
    FOREIGN_STATE_WRITE = 4006, "foreign-state-write", Error,
        "Only a service's own methods can write its fields.";
    /// A server-only declaration is used by code that reaches the browser.
    SERVER_ONLY_IN_CLIENT = 4007, "server-only-in-client", Error,
        "A server-only declaration is used by code that reaches the browser.";
    /// A service is server-only and client-only at once.
    SERVICE_ON_BOTH_SIDES = 4008, "service-on-both-sides", Error,
        "A service is server-only and client-only at once.";
    /// A job's dependency graph reaches `Session` or `SessionManager`.
    JOB_REACHES_SESSION = 4009, "job-reaches-session", Error,
        "A job's dependency graph reaches `Session` or `SessionManager`.";
    /// In `use X for Y`, `X` must be `Y` or implement it.
    USE_TARGET_MISMATCH = 4010, "use-target-mismatch", Error,
        "In `use X for Y`, `X` must be `Y` or implement it.";
    /// `use <expression> for C` is allowed only in a test body, for a `config` type `C`.
    USE_EXPRESSION_NOT_ALLOWED = 4011, "use-expression-not-allowed", Error,
        "`use <expression> for C` is allowed only in a test body, for a `config` type `C`.";
    /// A client-only interface is called from code that can run during server rendering.
    CLIENT_ONLY_DURING_SSR = 4012, "client-only-during-ssr", Error,
        "A client-only interface is called from code that can run during server rendering.";
    /// A server-rendered page reads a client-only service's state, so the first paint shows its initial state.
    CLIENT_STATE_IN_SERVER_RENDER = 4013, "client-state-in-server-render", Warning,
        "A server-rendered page reads a client-only service's state, so the first paint shows its initial state.";
    /// A `use` rule in a test restates the test default, so it can be removed.
    USE_RESTATES_DEFAULT = 4014, "use-restates-default", Warning,
        "A `use` rule in a test restates the test default, so it can be removed.";
    /// The `use … in` rules for an interface leave an environment without an implementation.
    ENVIRONMENT_NOT_COVERED = 4015, "environment-not-covered", Error,
        "The `use … in` rules for an interface leave an environment without an implementation.";
    /// A service declares at most one `init()`, without parameters and without `throws`.
    INVALID_SERVICE_INIT = 4016, "invalid-service-init", Error,
        "A service declares at most one `init()`, without parameters and without `throws`.";
    /// A config holds only `@Env` fields.
    CONFIG_MEMBER_NOT_ALLOWED = 4017, "config-member-not-allowed", Error,
        "A config holds only `@Env` fields.";
    /// Two config fields read the same environment variable.
    DUPLICATE_ENV_VARIABLE = 4018, "duplicate-env-variable", Error,
        "Two config fields read the same environment variable.";
    /// A config field's type must implement `ConfigValue`.
    NOT_A_CONFIG_VALUE = 4019, "not-a-config-value", Error,
        "A config field's type must implement `ConfigValue`.";
    /// A config can't be constructed outside test code; its values come from the environment.
    CONFIG_CONSTRUCTION = 4020, "config-construction", Error,
        "A config can't be constructed outside test code; its values come from the environment.";
    /// Only test code may implement a service (to fake it); production code uses interfaces.
    IMPLEMENTS_SERVICE = 4021, "implements-service", Error,
        "Only test code may implement a service (to fake it); production code uses interfaces.";
    /// An endpoint page whose dependencies reach `Session` or `SessionManager` must inject `Session` itself.
    ENDPOINT_SESSION_NOT_INJECTED = 4022, "endpoint-session-not-injected", Error,
        "An endpoint page whose dependencies reach `Session` or `SessionManager` must inject `Session` itself.";
    /// A protocol from a dependency that the app doesn't implement is injected without a `remote` rule.
    MISSING_REMOTE_RULE = 4023, "missing-remote-rule", Error,
        "A protocol from a dependency that the app doesn't implement needs a `use remote(Config) for Protocol` rule.";
    /// A `remote` rule names something other than a library protocol the app doesn't implement.
    REMOTE_RULE_NOT_ALLOWED = 4024, "remote-rule-not-allowed", Error,
        "A `remote` rule binds only a protocol from a library dependency that the app doesn't implement.";
    /// `remote(…)` names something that isn't a config implementing `RemoteService` with a `url` and a `token`.
    NOT_A_REMOTE_SERVICE = 4025, "not-a-remote-service", Error,
        "`remote(…)` takes a config that implements `RemoteService`, with `url: Url` and `token: Secret<String>`.";
    /// `RemoteClient.at` builds a client of a protocol, given by the type its result is used as.
    REMOTE_CLIENT_NOT_A_PROTOCOL = 4026, "remote-client-not-a-protocol", Error,
        "`RemoteClient.at` builds a client of a protocol: use it where a protocol type is expected.";
    /// `@Rendering(.server)` on a component: components render on the server by default.
    SERVER_RENDERING_ON_COMPONENT = 4027, "server-rendering-on-component", Error,
        "Only `@Rendering(.client)` applies to a component; the server is the default.";
    /// A client-rendered component's `loading` block reads a member other than a `@Prop`.
    LOADING_READS_COMPONENT_STATE = 4028, "loading-reads-component-state", Error,
        "A client-rendered component's `loading` block is rendered by the server, so it reads only `@Prop`s.";
    /// `@Rendering(.client)` on a component of an app that is already client-rendered.
    REDUNDANT_CLIENT_RENDERING = 4029, "redundant-client-rendering", Warning,
        "`@Rendering(.client)` on a component has no effect in an app that is already client-rendered.";
    /// A member that needs a user's action (`@UserAction`) is called, or passed, where there is none.
    USER_ACTION_REQUIRED = 4030, "user-action-required", Error,
        "A member the browser allows only during a user's action (`@UserAction`) is called outside the synchronous part of a click, submit or key handler.";
    /// A WebSocket handshake header is set in code that may run in the browser.
    HANDSHAKE_HEADER_ON_CLIENT = 4031, "handshake-header-on-client", Error,
        "A WebSocket handshake header (`WebSocketRequest`'s `headers`, `secretHeaders` or `bearerToken`) is set in code that may run in the browser, which can't send headers on a handshake.";
    /// A config field holds a secret or private key without `Secret<…>`.
    BARE_PRIVATE_KEY_IN_CONFIG = 4032, "bare-private-key-in-config", Error,
        "A config field holds a secret or private key (`AesGcmKey`, `Ed25519PrivateKey`, `EcdsaP256PrivateKey`, `X25519PrivateKey`) that isn't wrapped in `Secret<…>`.";
    /// A browser-only stdlib type (`MediaStream`, `MediaTrack`, `MediaController`, `AudioMeter`, `RecordingSession`, `PeerConnection`, `MediaSender`, `DataChannel`) is named in a server-only declaration.
    CLIENT_ONLY_ON_SERVER = 4033, "client-only-on-server", Error,
        "A browser-only stdlib type (`MediaStream`, `MediaTrack`, `MediaController`, `AudioMeter`, `RecordingSession`, `PeerConnection`, `MediaSender`, `DataChannel`) is named in a server-only declaration.";
    /// `AppUpdates` is injected in an app that isn't `@Offline`, where it never reports an update.
    APP_UPDATES_WITHOUT_OFFLINE = 4034, "app-updates-without-offline", Warning,
        "`AppUpdates` reports a newer build only in an `@Offline` app, whose service worker caches the build; without `@Offline` it is always `false`.";
    /// A `@Worker` service declares state, a field the page would read, `init()`, an effect, a scope, an rpc or a public static function.
    WORKER_MEMBER_NOT_ALLOWED = 4035, "worker-member-not-allowed", Error,
        "A `@Worker` service keeps no state, since its worker can be restarted at any call: besides its methods it has only `private` constants with initializers, computed properties and helpers, and no `init()`, `@Effect`, `rpc` or `@Scope`.";
    /// A `@Worker` service injects something.
    WORKER_INJECTION = 4036, "worker-injection", Error,
        "A `@Worker` service injects nothing: its worker has no container, so what a method needs is one of its arguments.";
    /// A `@Worker` service's method isn't `async throws E`, or is generic.
    WORKER_METHOD_SIGNATURE = 4037, "worker-method-signature", Error,
        "Every method of a `@Worker` service that isn't `private` is a call into the worker: `async`, `throws` an error enum, and not generic.";
    /// A worker method's error enum has no `workerFailure(WorkerFailure)` case.
    WORKER_ERROR_WITHOUT_FAILURE_CASE = 4038, "worker-error-without-failure-case", Error,
        "A worker method's error is `WorkerOnlyError` or an enum with exactly one `workerFailure(WorkerFailure)` case.";
    /// A worker method's parameter, result or error isn't `Codable`.
    WORKER_PAYLOAD_NOT_CODABLE = 4039, "worker-payload-not-codable", Error,
        "A worker method's parameters, result and error cross to another thread through their codecs, so they must be `Codable`.";

    // ---------------------------------------------------------------- pages and routing
    /// Two pages or endpoints have the same URL.
    URL_CONFLICT = 5001, "url-conflict", Error,
        "Two pages or endpoints have the same URL.";
    /// A `@Param` field has a type that can't be read from a URL.
    UNROUTABLE_PARAM = 5002, "unroutable-param", Error,
        "A `@Param` field has a type that can't be read from a URL.";
    /// An `at` template and the page's path `@Param` fields don't match one to one.
    AT_TEMPLATE_MISMATCH = 5003, "at-template-mismatch", Error,
        "An `at` template and the page's path `@Param` fields don't match one to one.";
    /// An endpoint page needs an `at` clause.
    ENDPOINT_WITHOUT_AT = 5004, "endpoint-without-at", Error,
        "An endpoint page needs an `at` clause.";
    /// `requires Authenticated` and `requires Role` don't apply to endpoint pages.
    GUARD_ON_ENDPOINT = 5005, "guard-on-endpoint", Error,
        "`requires Authenticated` and `requires Role` don't apply to endpoint pages.";
    /// A `requires` clause names something that doesn't implement `Guard`.
    NOT_A_GUARD = 5006, "not-a-guard", Error,
        "A `requires` clause names something that doesn't implement `Guard`.";
    /// A page `requires Authenticated`, but the app has no `@SignInPage`.
    MISSING_SIGN_IN_PAGE = 5007, "missing-sign-in-page", Error,
        "A page `requires Authenticated`, but the app has no `@SignInPage`.";
    /// The sign-in page can't itself `requires Authenticated`.
    SIGN_IN_PAGE_REQUIRES_AUTHENTICATED = 5008, "sign-in-page-requires-authenticated", Error,
        "The sign-in page can't itself `requires Authenticated`.";
    /// `@Layout` and `@NoLayout` don't apply to endpoint pages.
    LAYOUT_ON_ENDPOINT = 5009, "layout-on-endpoint", Error,
        "`@Layout` and `@NoLayout` don't apply to endpoint pages.";
    /// An endpoint page has handlers only: no `view`, `metadata`, `loading`, `failed` or `style`.
    ENDPOINT_MEMBER_NOT_ALLOWED = 5010, "endpoint-member-not-allowed", Error,
        "An endpoint page has handlers only: no `view`, `metadata`, `loading`, `failed` or `style`.";
    /// An endpoint page has at most one handler per HTTP method.
    DUPLICATE_HANDLER_METHOD = 5011, "duplicate-handler-method", Error,
        "An endpoint page has at most one handler per HTTP method.";
    /// An endpoint handler is `func name(request: IncomingRequest) async -> HttpResponse`.
    INVALID_HANDLER_SIGNATURE = 5012, "invalid-handler-signature", Error,
        "An endpoint handler is `func name(request: IncomingRequest) async -> HttpResponse`.";
    /// A response sets a literal `set-cookie` header; cookies are set only through `Session`.
    SET_COOKIE_HEADER = 5013, "set-cookie-header", Error,
        "A response sets a literal `set-cookie` header; cookies are set only through `Session`.";
    /// A layout has a `requires` clause, which the server never checks.
    GUARD_ON_LAYOUT = 5014, "guard-on-layout", Error,
        "A layout has a `requires` clause, which the server never checks: guards go on the pages.";

    // ---------------------------------------------------------------- protocols and rpc
    /// Every `rpc` declares `throws`: an error enum with a `rpcFailure(RpcFailure)` case, or `RpcOnlyError`.
    RPC_WITHOUT_THROWS = 5501, "rpc-without-throws", Error,
        "Every `rpc` declares `throws`: an error enum with a `rpcFailure(RpcFailure)` case, or `RpcOnlyError`.";
    /// An rpc's error enum needs exactly one `rpcFailure(RpcFailure)` case.
    RPC_ERROR_WITHOUT_FAILURE_CASE = 5502, "rpc-error-without-failure-case", Error,
        "An rpc's error enum needs exactly one `rpcFailure(RpcFailure)` case.";
    /// Rpc parameters, results and errors must be `Codable`.
    RPC_PAYLOAD_NOT_CODABLE = 5503, "rpc-payload-not-codable", Error,
        "Rpc parameters, results and errors must be `Codable`.";
    /// A protocol is implemented only by a server-side service.
    RPC_IMPLEMENTATION_NOT_SERVER = 5504, "rpc-implementation-not-server", Error,
        "A protocol is implemented only by a server-side service.";
    /// An rpc handler doesn't repeat its rpc's signature.
    RPC_HANDLER_MISMATCH = 5505, "rpc-handler-mismatch", Error,
        "An rpc handler doesn't repeat its rpc's signature.";
    /// A protocol handler throws the `rpcFailure` case, which only the runtime produces.
    HANDLER_THROWS_RPC_FAILURE = 5506, "handler-throws-rpc-failure", Error,
        "Only the runtime produces `rpcFailure`: a handler throws its error enum's own cases.";
    /// A `requires` clause on an rpc outside a protocol, which the server never checks.
    REQUIRES_ON_HANDLER = 5507, "requires-on-handler", Error,
        "A `requires` clause on a service's rpc is never checked: the server runs the guards the protocol's rpc declares.";
    /// An `rpc` member that no protocol its declaration implements declares.
    RPC_OUTSIDE_PROTOCOL = 5508, "rpc-outside-protocol", Error,
        "Only a protocol's rpcs are served: an `rpc` elsewhere is a `func`, or belongs in a protocol the service implements.";
    /// A `Channel` result anywhere but a protocol's rpc or the handler that serves it.
    CHANNEL_RESULT_MISPLACED = 5509, "channel-result-misplaced", Error,
        "A `Channel` is the result of a protocol's rpc, and of the handler that serves it, only; elsewhere it is held in a field or a local.";
    /// A `Channel` inside another type (`List<Channel<…>>`, `Stream<Channel<…>>`).
    NESTED_CHANNEL = 5510, "nested-channel", Error,
        "A `Channel` can't be inside another type: one channel per subject, held in a field or a local of type `Channel<C, S, E>` or `Channel<C, S, E>?`.";
    /// A channel is opened, or its client side used, by server-only code.
    CHANNEL_ON_SERVER = 5511, "channel-on-server", Error,
        "A channel's `state`, `messages`, `send` and `close`, and opening one, are the browser's: server-only code only builds a channel in a handler.";
    /// `Channel(sending:receiving:)` outside the handler of a channel rpc.
    CHANNEL_BUILT_OUTSIDE_HANDLER = 5512, "channel-built-outside-handler", Error,
        "Only the handler of a channel rpc builds a `Channel(sending:receiving:)`, and returns it.";
    /// A channel rpc is called from code that can run while the server renders.
    CHANNEL_OPENED_DURING_SSR = 5513, "channel-opened-during-ssr", Warning,
        "Channels open only in the browser: a channel rpc called from a `@Load` or a field initializer panics while the server renders; open it from an `@Effect`.";
    /// A protocol or an rpc declares generic parameters.
    GENERIC_RPC = 5514, "generic-rpc", Error,
        "A protocol and its rpcs can't be generic: `rpc.lock` records each rpc's concrete parameter, result and error types, which deployed callers rely on.";

    // ---------------------------------------------------------------- persistence
    /// A repository query lambda uses something that can't be translated to SQL.
    UNTRANSLATABLE_QUERY = 6001, "untranslatable-query", Error,
        "A repository query lambda uses something that can't be translated to SQL.";
    /// An entity field has a type that can't be stored in a column.
    UNSUPPORTED_COLUMN_TYPE = 6002, "unsupported-column-type", Error,
        "An entity field has a type that can't be stored in a column.";
    /// An entity's table name starts with `k_`, which is reserved for the stdlib.
    RESERVED_TABLE_PREFIX = 6003, "reserved-table-prefix", Error,
        "An entity's table name starts with `k_`, which is reserved for the stdlib.";
    /// A migration doesn't replay: it names a table, column or case that the earlier migrations don't leave, or calls something the schema builder doesn't have.
    INVALID_MIGRATION = 6004, "invalid-migration", Error,
        "A migration doesn't replay against the schema the earlier migrations leave.";
    /// A migration destroys data without `@Breaking("reason")`.
    DESTRUCTIVE_MIGRATION = 6005, "destructive-migration", Error,
        "A migration that drops a table or column, or renames or removes an enum case, needs `@Breaking(\"reason\")`.";
    /// A migration drops or renames something the previous release still reads.
    LIVE_SCHEMA_CHANGE = 6006, "live-schema-change", Error,
        "A migration drops or renames a table or column that no earlier migration retired; only `@Breaking(\"single instance\")` allows it.";

    // ---------------------------------------------------------------- forms and validation
    /// `form.value` is readable only inside the action passed to `form.handle(...)`.
    FORM_VALUE_OUTSIDE_HANDLE = 7001, "form-value-outside-handle", Error,
        "`form.value` is readable only inside the action passed to `form.handle(...)`.";
    /// `.empty` and `.empty(...)` are valid only as a `@Form` initializer.
    EMPTY_OUTSIDE_FORM = 7002, "empty-outside-form", Error,
        "`.empty` and `.empty(...)` are valid only as a `@Form` initializer.";
    /// `@Format(message:)` applies to fields of parsed types, not `String`.
    FORMAT_ON_STRING = 7003, "format-on-string", Error,
        "`@Format(message:)` applies to fields of parsed types, not `String`.";
    /// `@Email` on an `EmailAddress` field is redundant; the type already validates.
    REDUNDANT_EMAIL = 7004, "redundant-email", Warning,
        "`@Email` on an `EmailAddress` field is redundant; the type already validates.";
    /// `@Matches` takes a string literal, so the pattern can be checked at compile time.
    MATCHES_NOT_LITERAL = 7005, "matches-not-literal", Error,
        "`@Matches` takes a string literal, so the pattern can be checked at compile time.";
    /// A `@Matches` pattern is not a valid regular expression.
    INVALID_REGEX = 7006, "invalid-regex", Error,
        "A `@Matches` pattern is not a valid regular expression.";

    // ---------------------------------------------------------------- tests
    /// `expect` takes a `Bool` condition.
    EXPECT_NOT_BOOL = 7501, "expect-not-bool", Error,
        "`expect` takes a `Bool` condition.";
    /// The block of `expect throws E { … }` can't throw `E`.
    EXPECT_THROWS_CANNOT_THROW = 7502, "expect-throws-cannot-throw", Error,
        "The block of `expect throws E { … }` can't throw `E`.";
    /// A test group's dependency graph reaches an app config that no `use` rule binds.
    UNBOUND_TEST_CONFIG = 7503, "unbound-test-config", Error,
        "A test group's dependency graph reaches an app config that no `use` rule binds.";
    /// A test failed when `k test` or `k check` ran it: an `expect` was false, an `expect throws` block didn't throw a matching error, or the test panicked, threw or never finished.
    TEST_FAILED = 7510, "test-failed", Error,
        "A test failed when `k test` or `k check` ran it.";

    // ---------------------------------------------------------------- lockfiles, manifest and packages
    /// A change breaks a lockfile's recorded contract without `@Breaking`.
    BREAKING_CHANGE = 8001, "breaking-change", Error,
        "A change breaks a lockfile's recorded contract without `@Breaking`.";
    /// A lockfile can't be read.
    INVALID_LOCKFILE = 8002, "invalid-lockfile", Error,
        "A lockfile can't be read.";
    /// A job's payload changed incompatibly with the shape `storage.lock` records, without `@Breaking`.
    INCOMPATIBLE_JOB_PAYLOAD = 8003, "incompatible-job-payload", Error,
        "A job's payload changed incompatibly with `storage.lock`; queued jobs couldn't be decoded.";
    /// The entities differ from `schema.lock`.
    SCHEMA_DRIFT = 8004, "schema-drift", Error,
        "The entities differ from `schema.lock`; `k migrate new` writes the migration.";
    /// Replaying `migrations/` doesn't produce `schema.lock`.
    SCHEMA_LOCK_MISMATCH = 8005, "schema-lock-mismatch", Error,
        "Replaying `migrations/` doesn't produce the schema `schema.lock` records.";
    /// A migration file doesn't match its place in `schema.lock`'s `migrations` list.
    MIGRATION_ORDER = 8006, "migration-order", Error,
        "A migration file's number doesn't match its position in `schema.lock`'s `migrations` list.";
    /// A URL that `routes.lock` records is no longer served and has no redirect.
    REMOVED_URL = 8007, "removed-url", Error,
        "A URL `routes.lock` records is no longer served, and no redirect keeps it working.";
    /// A storage key's value type changed incompatibly with `storage.lock`, without `@Breaking`.
    INCOMPATIBLE_STORAGE_KEY = 8008, "incompatible-storage-key", Error,
        "A storage key changed incompatibly with `storage.lock`; stored values would no longer decode.";
    /// A collection changed incompatibly with `storage.lock`: its value shape or name without `@Breaking`, or its key field.
    INCOMPATIBLE_COLLECTION = 8009, "incompatible-collection", Error,
        "A collection changed incompatibly with `storage.lock`; the values users' browsers hold would no longer decode or be found.";
    /// A `KeyName` changed incompatibly with `storage.lock`: renamed without `@Breaking`, or given another key type.
    INCOMPATIBLE_KEY_NAME = 8010, "incompatible-key-name", Error,
        "A `KeyName` changed incompatibly with `storage.lock`; the keys users' browsers hold would no longer be found.";
    /// `k.toml` is malformed or lacks a required key.
    INVALID_MANIFEST = 8101, "invalid-manifest", Error,
        "`k.toml` is malformed or lacks a required key.";
    /// A dependency can't be resolved: `{ workspace = true }` names no member of the workspace.
    UNKNOWN_DEPENDENCY = 8102, "unknown-dependency", Error,
        "A dependency can't be resolved: `{ workspace = true }` names no member of the workspace.";
    /// A workspace member directory has no `k.toml`.
    MISSING_WORKSPACE_MEMBER = 8103, "missing-workspace-member", Error,
        "A workspace member listed in `members` has no `k.toml`.";
    /// Packages or workspace members depend on each other in a cycle.
    PACKAGE_CYCLE = 8104, "package-cycle", Error,
        "Packages depend on each other in a cycle; the dependency graph must be acyclic.";
    /// A dependency names an app, which can't be depended on.
    APP_DEPENDENCY = 8105, "app-dependency", Error,
        "Only libraries can be depended on; an app can't be a dependency.";
    /// Two packages of one build declare the same module.
    DUPLICATE_MODULE = 8106, "duplicate-module", Error,
        "Two packages of one build declare the same module.";
    /// A workspace member's `k` requirement allows no toolchain the root's `[workspace] k` allows.
    CONFLICTING_TOOLCHAIN = 8107, "conflicting-toolchain", Error,
        "A member's `k` allows no toolchain that the workspace's `[workspace] k` allows; a workspace builds with one toolchain.";
    /// A preview feature is used without the project listing it in `[package] preview`.
    PREVIEW_NOT_ENABLED = 8108, "preview-not-enabled", Error,
        "A preview feature is used, but the project doesn't list it in `[package] preview`; a preview may change in any minor, and listing it is the consent.";
    /// A dependency lists a preview feature that the depending project doesn't.
    DEPENDENCY_PREVIEW = 8109, "dependency-preview", Error,
        "A dependency uses a preview feature, so the project that depends on it must list that feature in `[package] preview` too.";
    /// `[package] preview` lists a feature that is stable now.
    STABILIZED_PREVIEW = 8110, "stabilized-preview", Warning,
        "`[package] preview` lists a feature that has become stable; the opt-in is no longer needed.";
    /// The running toolchain doesn't match the project's requirement or `k.lock`, and no installed one does.
    TOOLCHAIN_MISMATCH = 8201, "toolchain-mismatch", Error,
        "The project needs another toolchain than the running one, and it isn't installed.";
    /// A lockfile says `format = N` for a format newer than the running toolchain reads.
    LOCKFILE_TOO_NEW = 8202, "lockfile-too-new", Error,
        "A lockfile is in a format newer than this toolchain reads; a newer toolchain wrote it.";

    // --------------------------------------------------------------- driver
    /// A source file that is not valid UTF-8.
    INVALID_UTF8 = 9001, "invalid-utf8", Error,
        "A source file is not valid UTF-8.";
    /// A source file that could not be read.
    IO_ERROR = 9002, "io-error", Error,
        "A file or directory could not be read.";
    /// `k check` couldn't build or run the project's tests.
    TESTS_NOT_RUN = 9003, "tests-not-run", Error,
        "`k check` couldn't build or run the project's tests.";
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

/// The codes whose machine-applicable fixes migrate a project to a newer
/// toolchain (`docs/design/48-releases-and-versioning.md`, "Upgrading a
/// project"): `k upgrade` applies these fixes and no others, so the
/// upgrade's diff holds only the migration. A deprecation's warning, and
/// the tombstone error a removed form leaves for one minor, belong here;
/// a changelog fragment's `migration:` must name one of them
/// (`cargo test -p xtask` checks it).
pub const MIGRATIONS: &[Code] = &[DEPRECATED, STABILIZED_PREVIEW];

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
