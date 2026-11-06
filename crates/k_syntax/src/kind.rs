//! The kinds of tokens and nodes in a K syntax tree.
//!
//! [`SyntaxKind`] is the single list of every token kind and node kind. Its
//! variant names are part of the golden-test contract: they are written
//! verbatim in `.tokens` files (token kinds) and as the heads of the
//! S-expressions in `.ast` files (node kinds). Renaming a variant is a
//! breaking change to the test corpus.
//!
//! The authoritative description of when each token is produced and which
//! production builds each node is `docs/compiler/grammar.md`.
//!
//! # Layout
//!
//! The variants are declared in this order, and the classification methods
//! rely on it:
//!
//! 1. trivia (whitespace, newlines, comments);
//! 2. the error token;
//! 3. literal and identifier tokens;
//! 4. punctuation and operators;
//! 5. hard keywords (always keywords);
//! 6. contextual keywords (lexed as [`SyntaxKind::Ident`] and remapped by the
//!    parser where they act as keywords);
//! 7. nodes.

/// Expands the kind table into the enum and its lookup tables.
macro_rules! syntax_kinds {
    (
        tokens { $( $(#[doc = $tdoc:literal])* $tok:ident $(= $ttext:literal)? , )* }
        keywords { $( $(#[doc = $kdoc:literal])* $kw:ident = $ktext:literal , )* }
        contextual { $( $(#[doc = $cdoc:literal])* $ckw:ident = $ctext:literal , )* }
        nodes { $( $(#[doc = $ndoc:literal])* $node:ident , )* }
    ) => {
        /// The kind of a token or node in a K syntax tree.
        ///
        /// See the [module documentation](crate::kind) for the ordering and
        /// `docs/compiler/grammar.md` for the meaning of each kind.
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $( $(#[doc = $tdoc])* $tok, )*
            $( $(#[doc = $kdoc])* $kw, )*
            $( $(#[doc = $cdoc])* $ckw, )*
            $( $(#[doc = $ndoc])* $node, )*
        }

        impl SyntaxKind {
            /// Every kind, in declaration order. `ALL[k as usize] == k`.
            pub const ALL: &'static [SyntaxKind] = &[
                $( SyntaxKind::$tok, )*
                $( SyntaxKind::$kw, )*
                $( SyntaxKind::$ckw, )*
                $( SyntaxKind::$node, )*
            ];

            /// Returns the variant name, exactly as written in `.tokens` and
            /// `.ast` golden files, for example `"LBrace"` or
            /// `"ComponentDecl"`.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $( SyntaxKind::$tok => stringify!($tok), )*
                    $( SyntaxKind::$kw => stringify!($kw), )*
                    $( SyntaxKind::$ckw => stringify!($ckw), )*
                    $( SyntaxKind::$node => stringify!($node), )*
                }
            }

            /// Returns the text every token of this kind has, if it is fixed.
            ///
            /// Punctuation, operators and keywords have fixed text; identifiers,
            /// literals, trivia, the error token and nodes do not.
            /// [`SyntaxKind::InterpStart`] and [`SyntaxKind::InterpEnd`] report
            /// `{` and `}`; the string quote kinds report `"`.
            #[must_use]
            pub const fn fixed_text(self) -> Option<&'static str> {
                match self {
                    $( SyntaxKind::$tok => syntax_kinds!(@text $($ttext)?), )*
                    $( SyntaxKind::$kw => Some($ktext), )*
                    $( SyntaxKind::$ckw => Some($ctext), )*
                    $( SyntaxKind::$node => None, )*
                }
            }

            /// Returns the hard keyword spelled `text`, if any.
            ///
            /// Hard keywords can never be used as plain identifiers, but they
            /// may still appear as member names after `.`, as argument labels
            /// and as parameter labels (see `docs/compiler/grammar.md`).
            #[must_use]
            pub fn from_keyword(text: &str) -> Option<SyntaxKind> {
                match text {
                    $( $ktext => Some(SyntaxKind::$kw), )*
                    _ => None,
                }
            }

            /// Returns the contextual keyword spelled `text`, if any.
            ///
            /// The lexer never produces contextual keyword kinds; it produces
            /// [`SyntaxKind::Ident`]. The parser remaps an identifier to its
            /// contextual keyword kind where the grammar treats it as a
            /// keyword (for example `page` at the start of a declaration).
            #[must_use]
            pub fn from_contextual_keyword(text: &str) -> Option<SyntaxKind> {
                match text {
                    $( $ctext => Some(SyntaxKind::$ckw), )*
                    _ => None,
                }
            }

            /// Returns `true` for hard keywords, including the reserved
            /// keywords `as` and `is`.
            #[must_use]
            pub const fn is_keyword(self) -> bool {
                matches!(self, $( SyntaxKind::$kw )|*)
            }

            /// Returns `true` for contextual keywords.
            #[must_use]
            pub const fn is_contextual_keyword(self) -> bool {
                matches!(self, $( SyntaxKind::$ckw )|*)
            }

            /// Returns `true` for node kinds.
            #[must_use]
            pub const fn is_node(self) -> bool {
                matches!(self, $( SyntaxKind::$node )|*)
            }
        }
    };
    (@text) => { None };
    (@text $text:literal) => { Some($text) };
}

syntax_kinds! {
    tokens {
        // ------------------------------------------------------------ trivia
        /// A run of spaces and tabs, plus a byte-order mark at offset 0.
        /// Never contains a line terminator. Other whitespace characters
        /// (form feed, no-break space, ...) are [`SyntaxKind::Error`].
        Whitespace,
        /// One line terminator: `\n`, `\r\n` or a lone `\r`.
        Newline,
        /// A `//` comment up to (not including) the line terminator. A
        /// comment starting with exactly `///` is a [`SyntaxKind::DocComment`].
        LineComment,
        /// A `/* ... */` comment. Block comments nest.
        BlockComment,
        /// A `///` documentation comment up to the line terminator.
        DocComment,

        // ------------------------------------------------------------- error
        /// Text the lexer could not tokenize, such as `$` or `` ` ``. A run of
        /// consecutive invalid characters forms one token.
        Error,

        // ------------------------------------------------ literals and names
        /// An identifier or contextual keyword: ASCII letter or `_`, then
        /// ASCII letters, digits and `_`. A lone `_` is
        /// [`SyntaxKind::Underscore`]. In style blocks, identifiers may also
        /// contain `-` after the first character (`sans-serif`).
        Ident,
        /// An integer literal: `0`, `1_000`, `0xFF`, `0b1010`, `0o17`.
        IntLit,
        /// A floating-point literal: `1.5`, `0.25`, `1e9`, `2.5e-3`.
        FloatLit,
        /// A number with a unit suffix written directly after it: `16px`,
        /// `1.5rem`, `200ms`, `50%`, `45deg`. The suffix is validated by the
        /// parser, not the lexer.
        UnitLit,
        /// A color literal: `#` followed by alphanumerics, for example
        /// `#3B82F6`. Must be 3, 4, 6 or 8 hexadecimal digits.
        ColorLit,
        /// The opening `"` of a string literal.
        StringStart = "\"",
        /// Literal text inside a string, between escapes and interpolations.
        StringText,
        /// An escape inside a string: `\n`, `\r`, `\t`, `\0`, `\\`, `\"`,
        /// `\u{...}` or `{{` (a literal `{`).
        StringEscape,
        /// The closing `"` of a string literal.
        StringEnd = "\"",
        /// The `{` opening an interpolation, inside a string or a style value.
        InterpStart = "{",
        /// The `}` closing an interpolation.
        InterpEnd = "}",

        // ---------------------------------------------------- punctuation
        /// `(`
        LParen = "(",
        /// `)`
        RParen = ")",
        /// `[`
        LBrack = "[",
        /// `]`
        RBrack = "]",
        /// `{`
        LBrace = "{",
        /// `}`
        RBrace = "}",
        /// `,`
        Comma = ",",
        /// `:`
        Colon = ":",
        /// `;` (only meaningful in style blocks)
        Semicolon = ";",
        /// `.`
        Dot = ".",
        /// `..` (not an operator; reported as K0119)
        DotDot = "..",
        /// `..<` (half-open range)
        DotDotLt = "..<",
        /// `...` (closed range)
        DotDotDot = "...",
        /// `?` (optional type suffix)
        Question = "?",
        /// `?.` (optional chaining)
        QuestionDot = "?.",
        /// `??` (nil coalescing)
        QuestionQuestion = "??",
        /// `@` (annotations, style at-rules)
        At = "@",
        /// `->` (return types)
        Arrow = "->",
        /// `=>` (lambdas)
        FatArrow = "=>",
        /// `=`
        Eq = "=",
        /// `==`
        EqEq = "==",
        /// `!=`
        BangEq = "!=",
        /// `!` (not an operator in K; reported as K0106)
        Bang = "!",
        /// `<`
        Lt = "<",
        /// `<=`
        LtEq = "<=",
        /// `>`
        Gt = ">",
        /// `>=`
        GtEq = ">=",
        /// `+`
        Plus = "+",
        /// `-`
        Minus = "-",
        /// `*`
        Star = "*",
        /// `/`
        Slash = "/",
        /// `%` (remainder; directly after a number it is part of a
        /// [`SyntaxKind::UnitLit`])
        Percent = "%",
        /// `+=`
        PlusEq = "+=",
        /// `-=`
        MinusEq = "-=",
        /// `*=`
        StarEq = "*=",
        /// `/=`
        SlashEq = "/=",
        /// `%=`
        PercentEq = "%=",
        /// `&` (nesting selector in style blocks)
        Amp = "&",
        /// `&&` (not an operator in K; reported as K0104)
        AmpAmp = "&&",
        /// `|` (not an operator in K)
        Pipe = "|",
        /// `||` (not an operator in K; reported as K0105)
        PipePipe = "||",
        /// `~` (general sibling combinator in style blocks)
        Tilde = "~",
        /// `_` (wildcard pattern, unlabeled parameter)
        Underscore = "_",
    }
    keywords {
        /// `and`
        AndKw = "and",
        /// `as` (reserved)
        AsKw = "as",
        /// `async`
        AsyncKw = "async",
        /// `await`
        AwaitKw = "await",
        /// `bind`
        BindKw = "bind",
        /// `break`
        BreakKw = "break",
        /// `case`
        CaseKw = "case",
        /// `catch`
        CatchKw = "catch",
        /// `class`
        ClassKw = "class",
        /// `continue`
        ContinueKw = "continue",
        /// `default`
        DefaultKw = "default",
        /// `do`
        DoKw = "do",
        /// `else`
        ElseKw = "else",
        /// `enum`
        EnumKw = "enum",
        /// `expect`
        ExpectKw = "expect",
        /// `extend`
        ExtendKw = "extend",
        /// `false`
        FalseKw = "false",
        /// `for`
        ForKw = "for",
        /// `func`
        FuncKw = "func",
        /// `guard`
        GuardKw = "guard",
        /// `if`
        IfKw = "if",
        /// `implements`
        ImplementsKw = "implements",
        /// `import`
        ImportKw = "import",
        /// `in`
        InKw = "in",
        /// `init`
        InitKw = "init",
        /// `interface`
        InterfaceKw = "interface",
        /// `is` (reserved)
        IsKw = "is",
        /// `let`
        LetKw = "let",
        /// `match`
        MatchKw = "match",
        /// `nil`
        NilKw = "nil",
        /// `not`
        NotKw = "not",
        /// `or`
        OrKw = "or",
        /// `private`
        PrivateKw = "private",
        /// `public`
        PublicKw = "public",
        /// `requires`
        RequiresKw = "requires",
        /// `return`
        ReturnKw = "return",
        /// `rpc`
        RpcKw = "rpc",
        /// `self`
        SelfKw = "self",
        /// `static`
        StaticKw = "static",
        /// `struct`
        StructKw = "struct",
        /// `style` (also switches the lexer into style mode; see grammar.md)
        StyleKw = "style",
        /// `throw`
        ThrowKw = "throw",
        /// `throws`
        ThrowsKw = "throws",
        /// `true`
        TrueKw = "true",
        /// `try`
        TryKw = "try",
        /// `use`
        UseKw = "use",
        /// `var`
        VarKw = "var",
        /// `when`
        WhenKw = "when",
        /// `where`
        WhereKw = "where",
        /// `while`
        WhileKw = "while",
    }
    contextual {
        /// `app` at the start of a declaration.
        AppKw = "app",
        /// `at` in a page header.
        AtKw = "at",
        /// `component` at the start of a declaration.
        ComponentKw = "component",
        /// `config` at the start of a declaration.
        ConfigKw = "config",
        /// `entity` at the start of a declaration.
        EntityKw = "entity",
        /// `failed` at the start of a member followed by `(` or `{`.
        FailedKw = "failed",
        /// `key` after the iterable of a `for` loop.
        KeyKw = "key",
        /// `layout` at the start of a declaration.
        LayoutKw = "layout",
        /// `loading` at the start of a member followed by `{`.
        LoadingKw = "loading",
        /// `matches` after the name of a theme variant.
        MatchesKw = "matches",
        /// `metadata` at the start of a member followed by `{`.
        MetadataKw = "metadata",
        /// `migration` at the start of a declaration.
        MigrationKw = "migration",
        /// `page` at the start of a declaration.
        PageKw = "page",
        /// `proto` after `import`, followed by a string.
        ProtoKw = "proto",
        /// `protocol` at the start of a declaration.
        ProtocolKw = "protocol",
        /// `repository` at the start of a declaration.
        RepositoryKw = "repository",
        /// `service` at the start of a declaration.
        ServiceKw = "service",
        /// `test` at the start of a declaration or test member.
        TestKw = "test",
        /// `theme` at the start of a declaration.
        ThemeKw = "theme",
        /// `variant` at the start of a theme member.
        VariantKw = "variant",
        /// `view` at the start of a member followed by `{`.
        ViewKw = "view",
    }
    nodes {
        // --------------------------------------------------------- files
        /// The root of every tree: imports followed by items.
        SourceFile,
        /// `import path` or `import proto "file.proto"`.
        Import,
        /// The dotted/slashed path of an import.
        ImportPath,

        // ------------------------------------------------- declarations
        /// `component Name { ... }`
        ComponentDecl,
        /// `page Name requires ... at "..." { ... }`
        PageDecl,
        /// `layout Name { ... }`
        LayoutDecl,
        /// `service Name implements P { ... }`
        ServiceDecl,
        /// `protocol Name { ... }`
        ProtocolDecl,
        /// `entity Name { ... }`
        EntityDecl,
        /// `repository Name for Entity { ... }`
        RepositoryDecl,
        /// `struct Name { ... }`
        StructDecl,
        /// `enum Name { ... }`
        EnumDecl,
        /// `class Name { ... }`
        ClassDecl,
        /// `interface Name { ... }`
        InterfaceDecl,
        /// `theme Name { ... }`
        ThemeDecl,
        /// `test Subject { ... }`: a group of tests with DI overrides.
        TestSuiteDecl,
        /// `test "description" { ... }`: a single test.
        TestCaseDecl,
        /// `app Name { ... }` in `App.k`.
        AppDecl,
        /// `config Name { ... }`: typed settings read from the environment.
        ConfigDecl,
        /// `migration Name { ... }` in `migrations/`.
        MigrationDecl,
        /// `extend Type { ... }`
        ExtendDecl,
        /// `func name(...) ... { ... }`
        FuncDecl,
        /// `let`/`var` at item or member level, including computed properties.
        FieldDecl,
        /// `init(...) { ... }`
        InitDecl,
        /// `rpc name(...) ... requires ... { ... }`
        RpcDecl,
        /// `case name(...)` in an enum.
        EnumCase,
        /// One associated value of an enum case: `available: Int` or `T`.
        CaseField,
        /// `use Impl for Interface in .env, ...`
        UseDecl,
        /// `color primary: Color = #3B82F6` in a theme.
        ThemeToken,
        /// `variant dark matches .prefersDark { ... }` in a theme.
        ThemeVariant,
        /// `surface = #111827` in a theme or a theme variant.
        ThemeOverride,
        /// `view { ... }`
        ViewBlock,
        /// `style { ... }`
        StyleBlock,
        /// `loading { ... }`
        LoadingBlock,
        /// `failed(let error) { ... }`
        FailedBlock,
        /// `metadata { ... }`
        MetadataBlock,

        // ----------------------------------------------- decl components
        /// The `{ ... }` body of a declaration that holds members.
        MemberList,
        /// `@Name` or `@Name(args)`.
        Annotation,
        /// `(args)` of a call, annotation or guard.
        ArgList,
        /// One argument: optional label, optional `bind`, value.
        Arg,
        /// `<T, U>` on a declaration.
        GenericParamList,
        /// One generic parameter.
        GenericParam,
        /// `where T implements X, ...`
        WhereClause,
        /// `T implements X`
        WherePredicate,
        /// `implements A, B`
        ImplementsClause,
        /// `requires A, B(args)`
        RequiresClause,
        /// One guard in a `requires` clause.
        Guard,
        /// `at "/url/{param}"`
        AtClause,
        /// `for Entity` in a repository header.
        ForClause,
        /// `(params)` of a function, init or rpc.
        ParamList,
        /// One parameter: optional label, name, type, optional default.
        Param,
        /// `throws ErrorType`
        ThrowsClause,
        /// `-> Type`
        ReturnType,

        // ----------------------------------------------------------- types
        /// `Name`, `a.b.Name`, `List<T>`
        PathType,
        /// `<T, U>` after a type name.
        GenericArgList,
        /// `T?`
        OptionalType,
        /// `(A, B)`
        TupleType,
        /// `(A) async throws E -> B`
        FuncType,
        /// `(T)`, used to apply `?` to a function type.
        ParenType,

        // ------------------------------------------------------ statements
        /// `{ statements }`
        Block,
        /// `let`/`var` inside a block.
        LetStmt,
        /// `target = value`, `target += value`, ...
        AssignStmt,
        /// `return` or `return value`
        ReturnStmt,
        /// `throw value`
        ThrowStmt,
        /// `break`
        BreakStmt,
        /// `continue`
        ContinueStmt,
        /// `guard conditions else { ... }`
        GuardStmt,
        /// `while conditions { ... }`
        WhileStmt,
        /// `for pattern in iterable key k { ... }`
        ForStmt,
        /// `key expr` in a `for` loop.
        KeyClause,
        /// `do { ... } catch ... { ... }`
        DoStmt,
        /// `catch patterns where guard { ... }`
        CatchClause,
        /// `expect condition`
        ExpectStmt,
        /// `expect throws Pattern { ... }`
        ExpectThrowsStmt,
        /// `let name` or `let name = value` in a condition list.
        LetCondition,

        // ----------------------------------------------------- expressions
        /// A number, unit, color, boolean or `nil` literal.
        Literal,
        /// A string literal with its text, escapes and interpolations.
        StringLit,
        /// `{expr}` inside a string or a style value.
        Interpolation,
        /// A reference to a name, or `self`.
        NameExpr,
        /// `.name`: resolved against the expected type.
        ImplicitMemberExpr,
        /// `(expr)`
        ParenExpr,
        /// `(a, b)`
        TupleExpr,
        /// `[a, b]`
        ListExpr,
        /// `[k: v]` or `[:]`
        MapExpr,
        /// `k: v` in a map literal.
        MapEntry,
        /// `{ params => statements }`
        LambdaExpr,
        /// The parameters of a lambda, up to and including `=>`.
        LambdaParamList,
        /// One lambda parameter, optionally typed.
        LambdaParam,
        /// `if conditions { ... } else ...`
        IfExpr,
        /// `match scrutinee { arms }`
        MatchExpr,
        /// `case patterns where guard { ... }` or `default { ... }`
        MatchArm,
        /// `where condition` on a match arm or catch clause.
        MatchGuard,
        /// `try expr` or `try expr else { ... }`
        TryExpr,
        /// `await expr`
        AwaitExpr,
        /// `-expr` or `not expr`
        PrefixExpr,
        /// `lhs op rhs`
        BinaryExpr,
        /// `callee(args) { trailing lambda }`
        CallExpr,
        /// `receiver.name` or `receiver?.name`
        MemberExpr,
        /// `receiver[index]`
        IndexExpr,
        /// `bind target` in a call argument.
        BindExpr,
        /// `value when condition` in a call argument or list element.
        WhenExpr,

        // -------------------------------------------------------- patterns
        /// `_`
        WildcardPattern,
        /// `let name`
        BindingPattern,
        /// A bare name, only in `for` loop patterns.
        IdentPattern,
        /// A literal, optionally negated.
        LiteralPattern,
        /// `lo..<hi` or `lo...hi`
        RangePattern,
        /// `.case`, `Type.case`, with optional associated patterns.
        EnumCasePattern,
        /// `label: pattern` inside an enum case pattern.
        PatternField,
        /// `(a, b)`
        TuplePattern,

        // ---------------------------------------------------------- styles
        /// `selectors { items }`
        StyleRule,
        /// One complex selector: compounds separated by combinators.
        Selector,
        /// Simple selectors written without whitespace between them.
        CompoundSelector,
        /// `.name` in a selector.
        ClassSelector,
        /// `&`
        NestingSelector,
        /// `:name` or `:name(args)`
        PseudoClass,
        /// `::name`
        PseudoElement,
        /// `[name]` or `[name = value]`
        AttributeSelector,
        /// `*`
        UniversalSelector,
        /// An element name in a selector (always an error, K0116).
        TypeSelector,
        /// `@name prelude { items }` or `@name prelude;`
        AtRule,
        /// The prelude of an at-rule.
        AtPrelude,
        /// `(name: value)` or `(name)` in an at-rule prelude.
        MediaFeature,
        /// `@keyframes name { blocks }`
        KeyframesRule,
        /// `selectors { declarations }` inside `@keyframes`.
        KeyframeBlock,
        /// `from`, `to` or a percentage before a keyframe block.
        KeyframeSelector,
        /// `property: value;`
        Declaration,
        /// One comma-separated group of space-separated style values.
        CssValue,
        /// `name(args)` in a style value.
        CssFunction,

        // ----------------------------------------------------------- error
        /// Tokens the parser skipped while recovering from an error.
        ErrorNode,
    }
}

impl SyntaxKind {
    /// Converts a raw kind number back into a `SyntaxKind`.
    ///
    /// Unknown numbers map to [`SyntaxKind::Error`]. They cannot occur in
    /// trees built by this crate; the fallback keeps conversion total.
    #[must_use]
    pub fn from_raw(raw: u16) -> SyntaxKind {
        SyntaxKind::ALL
            .get(usize::from(raw))
            .copied()
            .unwrap_or(SyntaxKind::Error)
    }

    /// Returns `true` for tokens (everything that is not a node).
    #[must_use]
    pub const fn is_token(self) -> bool {
        !self.is_node()
    }

    /// Returns `true` for whitespace, newlines and comments.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace
                | SyntaxKind::Newline
                | SyntaxKind::LineComment
                | SyntaxKind::BlockComment
                | SyntaxKind::DocComment
        )
    }

    /// Returns `true` for the reserved keywords `as` and `is`, which have no
    /// meaning yet and cannot be used as names.
    #[must_use]
    pub const fn is_reserved(self) -> bool {
        matches!(self, SyntaxKind::AsKw | SyntaxKind::IsKw)
    }

    /// Returns `true` for kinds that may be used as a member name after `.`,
    /// as an argument label or as a parameter label: identifiers, hard
    /// keywords and contextual keywords.
    #[must_use]
    pub const fn is_name_like(self) -> bool {
        matches!(self, SyntaxKind::Ident) || self.is_keyword() || self.is_contextual_keyword()
    }

    /// Returns `true` for tokens that the S-expression dump omits because the
    /// tree shape already conveys them: trivia, brackets, commas, `;`, `:`,
    /// `.`, `?`, `@`, `->`, `=>`, string quotes and interpolation braces.
    ///
    /// This list is part of the `.ast` golden-file contract; see
    /// `docs/compiler/grammar.md`, "Syntax tree dump format".
    #[must_use]
    pub const fn is_dump_omitted(self) -> bool {
        self.is_trivia()
            || matches!(
                self,
                SyntaxKind::LParen
                    | SyntaxKind::RParen
                    | SyntaxKind::LBrack
                    | SyntaxKind::RBrack
                    | SyntaxKind::LBrace
                    | SyntaxKind::RBrace
                    | SyntaxKind::Comma
                    | SyntaxKind::Semicolon
                    | SyntaxKind::Colon
                    | SyntaxKind::Dot
                    | SyntaxKind::Question
                    | SyntaxKind::At
                    | SyntaxKind::Arrow
                    | SyntaxKind::FatArrow
                    | SyntaxKind::StringStart
                    | SyntaxKind::StringEnd
                    | SyntaxKind::InterpStart
                    | SyntaxKind::InterpEnd
            )
    }

    /// Looks a kind up by its variant name, as written in golden files.
    ///
    /// This is a linear scan and is meant for test tooling, not hot paths.
    #[must_use]
    pub fn from_name(name: &str) -> Option<SyntaxKind> {
        SyntaxKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.name() == name)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_is_in_discriminant_order() {
        for (index, kind) in SyntaxKind::ALL.iter().enumerate() {
            assert_eq!(*kind as usize, index, "{kind:?}");
            assert_eq!(SyntaxKind::from_raw(index as u16), *kind);
        }
        assert_eq!(SyntaxKind::from_raw(u16::MAX), SyntaxKind::Error);
    }

    #[test]
    fn names_are_unique_and_round_trip() {
        let names: HashSet<_> = SyntaxKind::ALL.iter().map(|k| k.name()).collect();
        assert_eq!(names.len(), SyntaxKind::ALL.len());
        for kind in SyntaxKind::ALL {
            assert_eq!(SyntaxKind::from_name(kind.name()), Some(*kind));
        }
        assert_eq!(SyntaxKind::from_name("NoSuchKind"), None);
    }

    #[test]
    fn keywords_round_trip_through_their_text() {
        for kind in SyntaxKind::ALL.iter().copied().filter(|k| k.is_keyword()) {
            let text = kind.fixed_text().unwrap();
            assert_eq!(SyntaxKind::from_keyword(text), Some(kind));
            assert_eq!(
                SyntaxKind::from_contextual_keyword(text),
                None,
                "{text} is both"
            );
            assert!(kind.name().ends_with("Kw"));
        }
        for kind in SyntaxKind::ALL
            .iter()
            .copied()
            .filter(|k| k.is_contextual_keyword())
        {
            let text = kind.fixed_text().unwrap();
            assert_eq!(SyntaxKind::from_contextual_keyword(text), Some(kind));
            assert_eq!(SyntaxKind::from_keyword(text), None);
        }
        assert_eq!(SyntaxKind::from_keyword("component"), None);
        assert_eq!(SyntaxKind::from_keyword("Func"), None);
    }

    #[test]
    fn classification_is_a_partition() {
        for kind in SyntaxKind::ALL.iter().copied() {
            let classes = [
                kind.is_trivia(),
                kind == SyntaxKind::Error,
                kind.is_keyword(),
                kind.is_contextual_keyword(),
                kind.is_node(),
            ];
            assert!(classes.iter().filter(|c| **c).count() <= 1, "{kind:?}");
            assert_eq!(kind.is_token(), !kind.is_node());
        }
        assert!(SyntaxKind::SourceFile.is_node());
        assert!(SyntaxKind::ErrorNode.is_node());
        assert!(!SyntaxKind::Ident.is_node());
    }

    #[test]
    fn punctuation_has_fixed_text_and_literals_do_not() {
        assert_eq!(SyntaxKind::DotDotLt.fixed_text(), Some("..<"));
        assert_eq!(SyntaxKind::Ident.fixed_text(), None);
        assert_eq!(SyntaxKind::StringText.fixed_text(), None);
        assert_eq!(SyntaxKind::Block.fixed_text(), None);
    }

    #[test]
    fn dump_omits_only_tokens() {
        for kind in SyntaxKind::ALL.iter().copied() {
            if kind.is_dump_omitted() {
                assert!(kind.is_token(), "{kind:?}");
            }
        }
        assert!(!SyntaxKind::QuestionDot.is_dump_omitted());
        assert!(!SyntaxKind::Eq.is_dump_omitted());
    }

    #[test]
    fn name_like_kinds() {
        assert!(SyntaxKind::Ident.is_name_like());
        assert!(SyntaxKind::ClassKw.is_name_like());
        assert!(SyntaxKind::ViewKw.is_name_like());
        assert!(!SyntaxKind::IntLit.is_name_like());
    }
}
