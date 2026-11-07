//! Typed accessors over the lossless syntax tree.
//!
//! Every node kind in [`SyntaxKind`] has a zero-cost typed wrapper of the
//! same name (for example [`ComponentDecl`] for
//! [`SyntaxKind::ComponentDecl`]). Wrappers are created with
//! [`AstNode::cast`] and expose the node's parts through accessor methods
//! that return `Option`s, because the tree of an invalid program may lack any
//! part. Accessors never panic.
//!
//! Groups of related node kinds are collected into enums: [`Item`],
//! [`Member`], [`Stmt`], [`Expr`], [`Type`], [`Pattern`], [`CssItem`] and
//! [`SimpleSelector`].
//!
//! Common capabilities are traits: [`HasName`], [`HasAnnotations`],
//! [`HasVisibility`] and [`HasMemberList`].
//!
//! The shape of each node (which children it has, in which order) is
//! specified by the grammar productions in `docs/compiler/grammar.md`; the
//! production name equals the node kind name.
//!
//! # Example
//!
//! ```
//! use k_diagnostics::FileId;
//! use k_syntax::ast::{HasMemberList, HasName, Item, Member};
//!
//! let source = "component Counter {\n    @State var count: Int = 0\n    func increment() { count += 1 }\n}\n";
//! let file = k_syntax::parse(FileId::new(0), source).tree().expect("always a SourceFile");
//!
//! let Some(Item::ComponentDecl(counter)) = file.items().next() else { panic!("a component") };
//! assert_eq!(counter.name().map(|t| t.text().to_owned()).as_deref(), Some("Counter"));
//!
//! let functions: Vec<String> = counter
//!     .members()
//!     .into_iter()
//!     .filter_map(|member| match member {
//!         Member::FuncDecl(func) => func.name().map(|t| t.text().to_owned()),
//!         _ => None,
//!     })
//!     .collect();
//! assert_eq!(functions, ["increment"]);
//! ```

pub use rowan::ast::{AstChildren, AstNode};

use crate::kind::SyntaxKind;
use crate::syntax::{KLanguage, SyntaxNode, SyntaxToken};
use rowan::TextRange;

/// Helper functions for writing accessors.
pub mod support {
    use super::*;

    /// Returns the first child node of `parent` that casts to `N`.
    #[must_use]
    pub fn child<N: AstNode<Language = KLanguage>>(parent: &SyntaxNode) -> Option<N> {
        rowan::ast::support::child(parent)
    }

    /// Returns all child nodes of `parent` that cast to `N`.
    #[must_use]
    pub fn children<N: AstNode<Language = KLanguage>>(parent: &SyntaxNode) -> AstChildren<N> {
        rowan::ast::support::children(parent)
    }

    /// Returns the first child token of `parent` of the given kind.
    #[must_use]
    pub fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
        token_where(parent, |k| k == kind)
    }

    /// Returns `true` if `parent` has a direct child token of `kind`.
    ///
    /// Same answer as `token(parent, kind).is_some()`, but it reads the
    /// green tree directly, so it allocates no red nodes or tokens: the
    /// semantic pass asks this of every declaration and member on every
    /// revision (`docs/compiler/performance.md` §10.4).
    #[must_use]
    pub fn has_token(parent: &SyntaxNode, kind: SyntaxKind) -> bool {
        token_kind_where(parent, |k| k == kind).is_some()
    }

    /// Returns the kind of the first direct child token of `parent` that
    /// satisfies `predicate`, reading the green tree (no allocation; see
    /// [`has_token`]).
    #[must_use]
    pub fn token_kind_where(
        parent: &SyntaxNode,
        predicate: impl Fn(SyntaxKind) -> bool,
    ) -> Option<SyntaxKind> {
        parent
            .green()
            .children()
            .filter_map(rowan::NodeOrToken::into_token)
            .map(|t| <KLanguage as rowan::Language>::kind_from_raw(t.kind()))
            .find(|k| predicate(*k))
    }

    /// Returns the first child token of `parent` satisfying `predicate`.
    ///
    /// The children are scanned in the green tree, which allocates nothing;
    /// only the token found is materialized as a red token (rowan allocates
    /// one red node per child visited through `children_with_tokens`, which
    /// made these lookups the largest cost of rebuilding the item tree,
    /// `docs/compiler/performance.md` §10.4).
    #[must_use]
    pub fn token_where(
        parent: &SyntaxNode,
        predicate: impl Fn(SyntaxKind) -> bool,
    ) -> Option<SyntaxToken> {
        let (_, range) = token_kinds(parent).find(|(k, _)| predicate(*k))?;
        token_at(parent, range)
    }

    /// Returns all non-trivia child tokens of `parent`.
    pub fn tokens(parent: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
        parent
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
    }

    /// The kinds and ranges of `parent`'s direct child tokens, in order,
    /// read from the green tree (no allocation per child; see
    /// [`token_where`]). [`token_at`] materializes one of them.
    pub fn token_kinds(parent: &SyntaxNode) -> impl Iterator<Item = (SyntaxKind, TextRange)> + '_ {
        let mut offset = parent.text_range().start();
        parent.green().children().filter_map(move |child| {
            let range = TextRange::at(offset, child.text_len());
            offset = range.end();
            child.into_token().map(|t| {
                (
                    <KLanguage as rowan::Language>::kind_from_raw(t.kind()),
                    range,
                )
            })
        })
    }

    /// The direct child token of `parent` at exactly `range` (one of
    /// [`token_kinds`]), materialized as a red token.
    #[must_use]
    pub fn token_at(parent: &SyntaxNode, range: TextRange) -> Option<SyntaxToken> {
        let found = (!range.is_empty())
            .then(|| parent.child_or_token_at_range(range))
            .flatten()
            .and_then(rowan::NodeOrToken::into_token)
            .filter(|t| t.text_range() == range);
        found.or_else(|| {
            parent
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.text_range() == range)
        })
    }
}

/// Defines one typed wrapper per node kind.
macro_rules! ast_nodes {
    ($( $name:ident ),* $(,)?) => {
        $(
            #[doc = concat!("A typed view of a [`SyntaxKind::", stringify!($name), "`] node.")]
            ///
            /// See `docs/compiler/grammar.md` for the production that builds it.
            #[derive(Clone, PartialEq, Eq, Hash, Debug)]
            pub struct $name {
                syntax: SyntaxNode,
            }

            impl AstNode for $name {
                type Language = KLanguage;

                fn can_cast(kind: SyntaxKind) -> bool {
                    kind == SyntaxKind::$name
                }

                fn cast(node: SyntaxNode) -> Option<$name> {
                    Self::can_cast(node.kind()).then_some($name { syntax: node })
                }

                fn syntax(&self) -> &SyntaxNode {
                    &self.syntax
                }
            }
        )*

        /// Every node kind that has a typed wrapper, for exhaustiveness tests.
        #[cfg(test)]
        const WRAPPED_KINDS: &[SyntaxKind] = &[$( SyntaxKind::$name ),*];
    };
}

ast_nodes! {
    SourceFile, Import, ImportPath,
    ComponentDecl, PageDecl, LayoutDecl, ServiceDecl, ProtocolDecl, EntityDecl, RepositoryDecl,
    StructDecl, EnumDecl, ClassDecl, InterfaceDecl, ThemeDecl, TestSuiteDecl, TestCaseDecl,
    AppDecl, ConfigDecl, MigrationDecl, ExtendDecl, FuncDecl, FieldDecl, InitDecl, RpcDecl,
    EnumCase, CaseField, UseDecl, ThemeToken, ThemeVariant, ThemeOverride, ViewBlock, StyleBlock, LoadingBlock, FailedBlock,
    MetadataBlock,
    MemberList, Annotation, ArgList, Arg, GenericParamList, GenericParam, WhereClause,
    WherePredicate, ImplementsClause, RequiresClause, Guard, AtClause, ForClause, ParamList, Param,
    ThrowsClause, ReturnType,
    PathType, GenericArgList, OptionalType, TupleType, FuncType, ParenType,
    Block, LetStmt, AssignStmt, ReturnStmt, ThrowStmt, BreakStmt, ContinueStmt, GuardStmt,
    WhileStmt, ForStmt, KeyClause, DoStmt, CatchClause, ExpectStmt, ExpectThrowsStmt, LetCondition,
    Literal, StringLit, Interpolation, NameExpr, ImplicitMemberExpr, ParenExpr, TupleExpr,
    ListExpr, MapExpr, MapEntry, LambdaExpr, LambdaParamList, LambdaParam, IfExpr, MatchExpr,
    MatchArm, MatchGuard, TryExpr, AwaitExpr, PrefixExpr, BinaryExpr, CallExpr, MemberExpr,
    IndexExpr, BindExpr, WhenExpr,
    WildcardPattern, BindingPattern, IdentPattern, LiteralPattern, RangePattern, EnumCasePattern,
    PatternField, TuplePattern,
    StyleRule, Selector, CompoundSelector, ClassSelector, NestingSelector, PseudoClass,
    PseudoElement, AttributeSelector, UniversalSelector, TypeSelector, AtRule, AtPrelude,
    MediaFeature, KeyframesRule, KeyframeBlock, KeyframeSelector, Declaration, CssValue,
    CssFunction,
    ErrorNode,
}

/// Defines an enum over several node wrappers.
macro_rules! ast_enum {
    ($(#[doc = $doc:literal])* $name:ident { $( $variant:ident ),* $(,)? }) => {
        $(#[doc = $doc])*
        #[derive(Clone, PartialEq, Eq, Hash, Debug)]
        pub enum $name {
            $(
                #[doc = concat!("A [`", stringify!($variant), "`].")]
                $variant($variant),
            )*
        }

        impl AstNode for $name {
            type Language = KLanguage;

            fn can_cast(kind: SyntaxKind) -> bool {
                matches!(kind, $( SyntaxKind::$variant )|*)
            }

            fn cast(node: SyntaxNode) -> Option<$name> {
                match node.kind() {
                    $( SyntaxKind::$variant => Some($name::$variant($variant { syntax: node })), )*
                    _ => None,
                }
            }

            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $( $name::$variant(node) => node.syntax(), )*
                }
            }
        }
    };
}

ast_enum! {
    /// A top-level declaration of a file.
    Item {
        ComponentDecl, PageDecl, LayoutDecl, ServiceDecl, ProtocolDecl, EntityDecl,
        RepositoryDecl, StructDecl, EnumDecl, ClassDecl, InterfaceDecl, ThemeDecl,
        TestSuiteDecl, TestCaseDecl, AppDecl, ConfigDecl, MigrationDecl, ExtendDecl, FuncDecl,
        FieldDecl,
    }
}

ast_enum! {
    /// A member of a declaration body ([`MemberList`]).
    ///
    /// The parser accepts every member kind in every body; semantic analysis
    /// reports members that are not allowed in a given kind of declaration.
    Member {
        FieldDecl, FuncDecl, InitDecl, RpcDecl, EnumCase, UseDecl, ThemeToken, ThemeVariant,
        ThemeOverride, ViewBlock, StyleBlock, LoadingBlock, FailedBlock, MetadataBlock, TestCaseDecl,
        TestSuiteDecl,
    }
}

ast_enum! {
    /// An expression.
    Expr {
        Literal, StringLit, NameExpr, ImplicitMemberExpr, ParenExpr, TupleExpr, ListExpr,
        MapExpr, LambdaExpr, IfExpr, MatchExpr, TryExpr, AwaitExpr, PrefixExpr, BinaryExpr,
        CallExpr, MemberExpr, IndexExpr, BindExpr, WhenExpr,
    }
}

ast_enum! {
    /// A type.
    Type { PathType, OptionalType, TupleType, FuncType, ParenType }
}

ast_enum! {
    /// A pattern in `match`, `catch`, `for`, `failed` and `expect throws`.
    Pattern {
        WildcardPattern, BindingPattern, IdentPattern, LiteralPattern, RangePattern,
        EnumCasePattern, TuplePattern,
    }
}

ast_enum! {
    /// An item inside a style block or style rule.
    CssItem { StyleRule, AtRule, KeyframesRule, Declaration }
}

ast_enum! {
    /// A simple selector inside a [`CompoundSelector`].
    SimpleSelector {
        ClassSelector, NestingSelector, PseudoClass, PseudoElement, AttributeSelector,
        UniversalSelector, TypeSelector,
    }
}

/// A statement inside a [`Block`], a [`LambdaExpr`] body or a view block.
///
/// An expression used as a statement is not wrapped in a node of its own; it
/// appears directly as [`Stmt::Expr`]. In a view block, expression
/// statements are the view nodes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Stmt {
    /// A [`LetStmt`].
    Let(LetStmt),
    /// An [`AssignStmt`].
    Assign(AssignStmt),
    /// A [`ReturnStmt`].
    Return(ReturnStmt),
    /// A [`ThrowStmt`].
    Throw(ThrowStmt),
    /// A [`BreakStmt`].
    Break(BreakStmt),
    /// A [`ContinueStmt`].
    Continue(ContinueStmt),
    /// A [`GuardStmt`].
    Guard(GuardStmt),
    /// A [`WhileStmt`].
    While(WhileStmt),
    /// A [`ForStmt`].
    For(ForStmt),
    /// A [`DoStmt`].
    Do(DoStmt),
    /// An [`ExpectStmt`].
    Expect(ExpectStmt),
    /// An [`ExpectThrowsStmt`].
    ExpectThrows(ExpectThrowsStmt),
    /// An expression statement.
    Expr(Expr),
}

impl AstNode for Stmt {
    type Language = KLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::LetStmt
                | SyntaxKind::AssignStmt
                | SyntaxKind::ReturnStmt
                | SyntaxKind::ThrowStmt
                | SyntaxKind::BreakStmt
                | SyntaxKind::ContinueStmt
                | SyntaxKind::GuardStmt
                | SyntaxKind::WhileStmt
                | SyntaxKind::ForStmt
                | SyntaxKind::DoStmt
                | SyntaxKind::ExpectStmt
                | SyntaxKind::ExpectThrowsStmt
        ) || Expr::can_cast(kind)
    }

    fn cast(node: SyntaxNode) -> Option<Stmt> {
        Some(match node.kind() {
            SyntaxKind::LetStmt => Stmt::Let(LetStmt { syntax: node }),
            SyntaxKind::AssignStmt => Stmt::Assign(AssignStmt { syntax: node }),
            SyntaxKind::ReturnStmt => Stmt::Return(ReturnStmt { syntax: node }),
            SyntaxKind::ThrowStmt => Stmt::Throw(ThrowStmt { syntax: node }),
            SyntaxKind::BreakStmt => Stmt::Break(BreakStmt { syntax: node }),
            SyntaxKind::ContinueStmt => Stmt::Continue(ContinueStmt { syntax: node }),
            SyntaxKind::GuardStmt => Stmt::Guard(GuardStmt { syntax: node }),
            SyntaxKind::WhileStmt => Stmt::While(WhileStmt { syntax: node }),
            SyntaxKind::ForStmt => Stmt::For(ForStmt { syntax: node }),
            SyntaxKind::DoStmt => Stmt::Do(DoStmt { syntax: node }),
            SyntaxKind::ExpectStmt => Stmt::Expect(ExpectStmt { syntax: node }),
            SyntaxKind::ExpectThrowsStmt => Stmt::ExpectThrows(ExpectThrowsStmt { syntax: node }),
            _ => Stmt::Expr(Expr::cast(node)?),
        })
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Stmt::Let(n) => n.syntax(),
            Stmt::Assign(n) => n.syntax(),
            Stmt::Return(n) => n.syntax(),
            Stmt::Throw(n) => n.syntax(),
            Stmt::Break(n) => n.syntax(),
            Stmt::Continue(n) => n.syntax(),
            Stmt::Guard(n) => n.syntax(),
            Stmt::While(n) => n.syntax(),
            Stmt::For(n) => n.syntax(),
            Stmt::Do(n) => n.syntax(),
            Stmt::Expect(n) => n.syntax(),
            Stmt::ExpectThrows(n) => n.syntax(),
            Stmt::Expr(n) => n.syntax(),
        }
    }
}

// ------------------------------------------------------------------- traits

/// A node that declares a name: the first [`SyntaxKind::Ident`] token among
/// its direct children.
///
/// Keywords, including remapped contextual keywords such as `component`, are
/// never `Ident` tokens, so the first identifier is always the declared name.
pub trait HasName: AstNode<Language = KLanguage> {
    /// Returns the name token, if present.
    fn name(&self) -> Option<SyntaxToken> {
        support::token(self.syntax(), SyntaxKind::Ident)
    }

    /// Returns the declared name as text, if present.
    fn name_text(&self) -> Option<String> {
        self.name().map(|token| token.text().to_owned())
    }
}

/// A node that may carry annotations such as `@State` or `@Scope(.request)`.
pub trait HasAnnotations: AstNode<Language = KLanguage> {
    /// Returns the annotations, in source order.
    fn annotations(&self) -> AstChildren<Annotation> {
        support::children(self.syntax())
    }

    /// Returns the first annotation named `name` (without the `@`).
    fn annotation(&self, name: &str) -> Option<Annotation> {
        self.annotations()
            .find(|a| a.name().is_some_and(|n| n.text() == name))
    }
}

/// The visibility a declaration was written with.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Visibility {
    /// `public`: visible to every module.
    Public,
    /// No modifier: visible within the same module (directory).
    Module,
    /// `private`: visible only inside the enclosing declaration.
    Private,
}

/// A node that may carry a visibility modifier.
pub trait HasVisibility: AstNode<Language = KLanguage> {
    /// Returns the declared visibility. When both modifiers are present
    /// (an error reported by the parser), the first one wins.
    fn visibility(&self) -> Visibility {
        match support::token_where(self.syntax(), |k| {
            matches!(k, SyntaxKind::PublicKw | SyntaxKind::PrivateKw)
        })
        .map(|t| t.kind())
        {
            Some(SyntaxKind::PublicKw) => Visibility::Public,
            Some(SyntaxKind::PrivateKw) => Visibility::Private,
            _ => Visibility::Module,
        }
    }
}

/// A declaration whose body is a [`MemberList`].
pub trait HasMemberList: AstNode<Language = KLanguage> {
    /// Returns the `{ ... }` body.
    fn member_list(&self) -> Option<MemberList> {
        support::child(self.syntax())
    }

    /// Returns the members of the body, in source order. Empty when the body
    /// is missing.
    fn members(&self) -> Vec<Member> {
        self.member_list()
            .map(|list| list.members().collect())
            .unwrap_or_default()
    }
}

/// Implements the accessor trait `$trait` (whose methods all have
/// default bodies) for each listed AST node type.
macro_rules! impl_traits {
    ($trait:ident: $( $name:ident ),* $(,)?) => {
        $( impl $trait for $name {} )*
    };
}

impl_traits!(HasName:
    ComponentDecl, PageDecl, LayoutDecl, ServiceDecl, ProtocolDecl, EntityDecl, RepositoryDecl,
    StructDecl, EnumDecl, ClassDecl, InterfaceDecl, ThemeDecl, TestSuiteDecl, AppDecl, ConfigDecl,
    MigrationDecl, FuncDecl, FieldDecl, RpcDecl, EnumCase, GenericParam, ThemeVariant,
);
impl_traits!(HasAnnotations:
    ComponentDecl, PageDecl, LayoutDecl, ServiceDecl, ProtocolDecl, EntityDecl, RepositoryDecl,
    StructDecl, EnumDecl, ClassDecl, InterfaceDecl, ThemeDecl, TestSuiteDecl, TestCaseDecl,
    AppDecl, ConfigDecl, MigrationDecl, ExtendDecl, FuncDecl, FieldDecl, InitDecl, RpcDecl,
    EnumCase,
);
impl_traits!(HasVisibility:
    ComponentDecl, PageDecl, LayoutDecl, ServiceDecl, ProtocolDecl, EntityDecl, RepositoryDecl,
    StructDecl, EnumDecl, ClassDecl, InterfaceDecl, ThemeDecl, AppDecl, ConfigDecl, MigrationDecl,
    FuncDecl, FieldDecl, InitDecl, RpcDecl,
);
impl_traits!(HasMemberList:
    ComponentDecl, PageDecl, LayoutDecl, ServiceDecl, ProtocolDecl, EntityDecl, RepositoryDecl,
    StructDecl, EnumDecl, ClassDecl, InterfaceDecl, ThemeDecl, TestSuiteDecl, AppDecl, ConfigDecl,
    MigrationDecl, ExtendDecl,
);

// ---------------------------------------------------------------- accessors

impl SourceFile {
    /// Returns the imports, in source order.
    #[must_use]
    pub fn imports(&self) -> AstChildren<Import> {
        support::children(&self.syntax)
    }

    /// Returns the top-level declarations, in source order.
    #[must_use]
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.syntax)
    }
}

impl Import {
    /// Returns the path of a declaration import (`import shop.cart.Cart`).
    #[must_use]
    pub fn path(&self) -> Option<ImportPath> {
        support::child(&self.syntax)
    }

    /// Returns the file of a proto import (`import proto "a/b.proto"`).
    #[must_use]
    pub fn proto_file(&self) -> Option<StringLit> {
        support::child(&self.syntax)
    }
}

impl ImportPath {
    /// Returns the path exactly as written, without trivia, for example
    /// `shop.catalog.Product` or `github.com/acme/charts.LineChart`.
    #[must_use]
    pub fn text(&self) -> String {
        support::tokens(&self.syntax)
            .map(|t| t.text().to_owned())
            .collect()
    }

    /// Returns the last segment: the name of the imported declaration.
    #[must_use]
    pub fn declaration_name(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax)
            .filter(|t| t.kind().is_name_like())
            .last()
    }
}

impl MemberList {
    /// Returns the members, in source order.
    #[must_use]
    pub fn members(&self) -> AstChildren<Member> {
        support::children(&self.syntax)
    }
}

impl Annotation {
    /// Returns the annotation name token (`State` in `@State`).
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::Ident)
    }

    /// Returns the argument list, if the annotation has one.
    #[must_use]
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }
}

impl ArgList {
    /// Returns the arguments, in source order.
    #[must_use]
    pub fn args(&self) -> AstChildren<Arg> {
        support::children(&self.syntax)
    }
}

impl Arg {
    /// Returns the label token of a labeled argument (`value` in
    /// `value: bind quantity`). Labels may be keywords (`for: id`).
    #[must_use]
    pub fn label(&self) -> Option<SyntaxToken> {
        let mut tokens = support::tokens(&self.syntax);
        let first = tokens.next()?;
        let second = tokens.next()?;
        (first.kind().is_name_like() && second.kind() == SyntaxKind::Colon).then_some(first)
    }

    /// Returns the argument value. For `bind x` this is a [`BindExpr`]; for
    /// `x when c` it is a [`WhenExpr`].
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

impl FieldDecl {
    /// Returns the `let` or `var` keyword token.
    #[must_use]
    pub fn binding_keyword(&self) -> Option<SyntaxToken> {
        support::token_where(&self.syntax, |k| {
            matches!(k, SyntaxKind::LetKw | SyntaxKind::VarKw)
        })
    }

    /// Returns `true` if the field is declared with `var`.
    #[must_use]
    pub fn is_var(&self) -> bool {
        support::token_kind_where(&self.syntax, |k| {
            matches!(k, SyntaxKind::LetKw | SyntaxKind::VarKw)
        }) == Some(SyntaxKind::VarKw)
    }

    /// Returns `true` if the field is declared `static`.
    #[must_use]
    pub fn is_static(&self) -> bool {
        support::has_token(&self.syntax, SyntaxKind::StaticKw)
    }

    /// Returns the declared type, if written.
    #[must_use]
    pub fn ty(&self) -> Option<Type> {
        support::child(&self.syntax)
    }

    /// Returns the initializer after `=`, if any.
    #[must_use]
    pub fn initializer(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }

    /// Returns the body of a computed property (`var x: T { ... }`), if any.
    #[must_use]
    pub fn getter(&self) -> Option<Block> {
        support::child(&self.syntax)
    }
}

impl FuncDecl {
    /// Returns `true` if the function is declared `static`.
    #[must_use]
    pub fn is_static(&self) -> bool {
        support::has_token(&self.syntax, SyntaxKind::StaticKw)
    }

    /// Returns `true` if the function is `async`.
    #[must_use]
    pub fn is_async(&self) -> bool {
        support::has_token(&self.syntax, SyntaxKind::AsyncKw)
    }

    /// Returns the generic parameters, if any.
    #[must_use]
    pub fn generic_param_list(&self) -> Option<GenericParamList> {
        support::child(&self.syntax)
    }

    /// Returns the parameter list.
    #[must_use]
    pub fn param_list(&self) -> Option<ParamList> {
        support::child(&self.syntax)
    }

    /// Returns the `throws` clause, if any.
    #[must_use]
    pub fn throws_clause(&self) -> Option<ThrowsClause> {
        support::child(&self.syntax)
    }

    /// Returns the `-> T` return type, if any.
    #[must_use]
    pub fn return_type(&self) -> Option<ReturnType> {
        support::child(&self.syntax)
    }

    /// Returns the `where` clause, if any.
    #[must_use]
    pub fn where_clause(&self) -> Option<WhereClause> {
        support::child(&self.syntax)
    }

    /// Returns the body. Interface and protocol requirements have none.
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        support::child(&self.syntax)
    }
}

impl RpcDecl {
    /// Returns the parameter list.
    #[must_use]
    pub fn param_list(&self) -> Option<ParamList> {
        support::child(&self.syntax)
    }

    /// Returns the `throws` clause, if any.
    #[must_use]
    pub fn throws_clause(&self) -> Option<ThrowsClause> {
        support::child(&self.syntax)
    }

    /// Returns the `-> T` return type, if any.
    #[must_use]
    pub fn return_type(&self) -> Option<ReturnType> {
        support::child(&self.syntax)
    }

    /// Returns the `requires` clause, if any.
    #[must_use]
    pub fn requires_clause(&self) -> Option<RequiresClause> {
        support::child(&self.syntax)
    }

    /// Returns the body of an implementation; `None` in a protocol.
    #[must_use]
    pub fn body(&self) -> Option<Block> {
        support::child(&self.syntax)
    }
}

impl ParamList {
    /// Returns the parameters, in source order.
    #[must_use]
    pub fn params(&self) -> AstChildren<Param> {
        support::children(&self.syntax)
    }
}

impl Param {
    /// The ranges of the name-like tokens before the `:`: the label and
    /// the name, or the name alone (green scan, no allocation per token).
    fn name_ranges(&self) -> Vec<TextRange> {
        support::token_kinds(&self.syntax)
            .filter(|(k, _)| !k.is_trivia())
            .take_while(|(k, _)| *k != SyntaxKind::Colon)
            .filter(|(k, _)| k.is_name_like() || *k == SyntaxKind::Underscore)
            .map(|(_, r)| r)
            .collect()
    }

    /// Returns the external label when it differs from the name
    /// (`for` in `for productId: ProductId`, or `_`).
    #[must_use]
    pub fn label(&self) -> Option<SyntaxToken> {
        match self.name_ranges().as_slice() {
            [label, _] => support::token_at(&self.syntax, *label),
            _ => None,
        }
    }

    /// Returns the parameter name used inside the body.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        let last = *self.name_ranges().last()?;
        support::token_at(&self.syntax, last)
    }

    /// Returns the parameter type.
    #[must_use]
    pub fn ty(&self) -> Option<Type> {
        support::child(&self.syntax)
    }

    /// Returns the default value after `=`, if any.
    #[must_use]
    pub fn default_value(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

impl ThrowsClause {
    /// Returns the thrown error type.
    #[must_use]
    pub fn error_type(&self) -> Option<Type> {
        support::child(&self.syntax)
    }
}

impl ReturnType {
    /// Returns the returned type.
    #[must_use]
    pub fn ty(&self) -> Option<Type> {
        support::child(&self.syntax)
    }
}

impl PageDecl {
    /// Returns the `requires` clause, if any.
    #[must_use]
    pub fn requires_clause(&self) -> Option<RequiresClause> {
        support::child(&self.syntax)
    }

    /// Returns the `at "/url"` clause, if any.
    #[must_use]
    pub fn at_clause(&self) -> Option<AtClause> {
        support::child(&self.syntax)
    }
}

impl ServiceDecl {
    /// Returns the `implements` clause, if any.
    #[must_use]
    pub fn implements_clause(&self) -> Option<ImplementsClause> {
        support::child(&self.syntax)
    }
}

impl RepositoryDecl {
    /// Returns the `for Entity` clause.
    #[must_use]
    pub fn for_clause(&self) -> Option<ForClause> {
        support::child(&self.syntax)
    }
}

impl EnumDecl {
    /// Returns the enum's cases, in source order.
    #[must_use]
    pub fn cases(&self) -> Vec<EnumCase> {
        self.members()
            .into_iter()
            .filter_map(|m| {
                if let Member::EnumCase(c) = m {
                    Some(c)
                } else {
                    None
                }
            })
            .collect()
    }
}

impl ImplementsClause {
    /// Returns the implemented interfaces, protocols or conformances.
    #[must_use]
    pub fn types(&self) -> AstChildren<PathType> {
        support::children(&self.syntax)
    }
}

impl RequiresClause {
    /// Returns the guards, in source order.
    #[must_use]
    pub fn guards(&self) -> AstChildren<Guard> {
        support::children(&self.syntax)
    }
}

impl AtClause {
    /// Returns the URL template string.
    #[must_use]
    pub fn template(&self) -> Option<StringLit> {
        support::child(&self.syntax)
    }
}

impl Block {
    /// Returns the statements, in source order.
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + use<> {
        self.syntax.children().filter_map(Stmt::cast)
    }
}

impl LambdaExpr {
    /// Returns the parameter list, if the lambda declares parameters.
    #[must_use]
    pub fn param_list(&self) -> Option<LambdaParamList> {
        support::child(&self.syntax)
    }

    /// Returns the body statements, in source order.
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + use<> {
        self.syntax.children().filter_map(Stmt::cast)
    }
}

impl Literal {
    /// Returns the literal token.
    #[must_use]
    pub fn token(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax).next()
    }
}

impl StringLit {
    /// Returns the interpolations, in source order.
    #[must_use]
    pub fn interpolations(&self) -> AstChildren<Interpolation> {
        support::children(&self.syntax)
    }

    /// Returns `true` if the string contains no interpolation.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.interpolations().next().is_none()
    }

    /// Returns the decoded value of a string without interpolations: text
    /// with escapes resolved. Returns `None` if the string is interpolated.
    #[must_use]
    pub fn plain_value(&self) -> Option<String> {
        if !self.is_plain() {
            return None;
        }
        let mut value = String::new();
        for token in support::tokens(&self.syntax) {
            match token.kind() {
                SyntaxKind::StringText => value.push_str(token.text()),
                SyntaxKind::StringEscape => crate::dump::decode_escape(token.text(), &mut value),
                _ => {}
            }
        }
        Some(value)
    }
}

impl Interpolation {
    /// Returns the interpolated expression.
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

impl NameExpr {
    /// Returns the referenced name token (an identifier or `self`).
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax).next()
    }
}

impl ImplicitMemberExpr {
    /// Returns the member name token after the leading `.`.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax).find(|t| t.kind().is_name_like())
    }
}

impl BinaryExpr {
    /// Returns the left operand.
    #[must_use]
    pub fn lhs(&self) -> Option<Expr> {
        support::children(&self.syntax).next()
    }

    /// Returns the right operand.
    #[must_use]
    pub fn rhs(&self) -> Option<Expr> {
        support::children(&self.syntax).nth(1)
    }

    /// Returns the operator token.
    #[must_use]
    pub fn op(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax).next()
    }
}

impl PrefixExpr {
    /// Returns the operator token (`-` or `not`).
    #[must_use]
    pub fn op(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax).next()
    }

    /// Returns the operand.
    #[must_use]
    pub fn operand(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

impl CallExpr {
    /// Returns the callee.
    #[must_use]
    pub fn callee(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }

    /// Returns the parenthesized argument list, if written.
    #[must_use]
    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.syntax)
    }

    /// Returns the trailing lambda, if any. It is the last child node and is
    /// distinct from the callee.
    #[must_use]
    pub fn trailing_lambda(&self) -> Option<LambdaExpr> {
        let mut children = self.syntax.children();
        children.next()?;
        children.filter_map(LambdaExpr::cast).last()
    }
}

impl MemberExpr {
    /// Returns the receiver expression.
    #[must_use]
    pub fn receiver(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }

    /// Returns the member name token.
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax)
            .filter(|t| t.kind().is_name_like())
            .last()
    }

    /// Returns `true` for optional chaining (`a?.b`).
    #[must_use]
    pub fn is_optional(&self) -> bool {
        support::has_token(&self.syntax, SyntaxKind::QuestionDot)
    }
}

impl PathType {
    /// Returns the name segments (`CartProtocol`, `v2` in `CartProtocol.v2`).
    #[must_use]
    pub fn segments(&self) -> Vec<SyntaxToken> {
        support::tokens(&self.syntax)
            .filter(|t| t.kind() == SyntaxKind::Ident)
            .collect()
    }

    /// Returns the generic arguments, if any.
    #[must_use]
    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        support::child(&self.syntax)
    }
}

impl GenericArgList {
    /// Returns the type arguments.
    #[must_use]
    pub fn types(&self) -> AstChildren<Type> {
        support::children(&self.syntax)
    }
}

impl OptionalType {
    /// Returns the wrapped type (`T` in `T?`).
    #[must_use]
    pub fn inner(&self) -> Option<Type> {
        support::child(&self.syntax)
    }
}

impl StyleBlock {
    /// Returns the top-level style items.
    #[must_use]
    pub fn items(&self) -> AstChildren<CssItem> {
        support::children(&self.syntax)
    }
}

impl StyleRule {
    /// Returns the comma-separated selectors of the rule.
    #[must_use]
    pub fn selectors(&self) -> AstChildren<Selector> {
        support::children(&self.syntax)
    }

    /// Returns the nested items (declarations, rules, at-rules).
    #[must_use]
    pub fn items(&self) -> AstChildren<CssItem> {
        support::children(&self.syntax)
    }
}

impl UseDecl {
    /// Returns the operand before `for` when it is a type (`use FakeApi for
    /// Api`).
    #[must_use]
    pub fn implementation(&self) -> Option<Type> {
        self.before_for().find_map(Type::cast)
    }

    /// Returns the operand before `for` when it is an expression: a value
    /// bound to a config in a test body (`use Config.sample for Config`).
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        self.before_for().find_map(Expr::cast)
    }

    /// Returns the type after `for`.
    #[must_use]
    pub fn target(&self) -> Option<Type> {
        self.after_for().find_map(Type::cast)
    }

    /// Returns the environments after `in` (`.production`, `.test`), in
    /// source order.
    #[must_use]
    pub fn environments(&self) -> Vec<Expr> {
        self.after_for().filter_map(Expr::cast).collect()
    }

    /// Returns `true` if the declaration has a `for` keyword.
    fn has_for(&self) -> bool {
        support::has_token(&self.syntax, SyntaxKind::ForKw)
    }

    /// The child nodes before the `for` token (all of them without one).
    fn before_for(&self) -> impl Iterator<Item = SyntaxNode> {
        self.syntax
            .children_with_tokens()
            .take_while(|e| e.kind() != SyntaxKind::ForKw)
            .filter_map(rowan::NodeOrToken::into_node)
    }

    /// The child nodes after the `for` token (none without one).
    fn after_for(&self) -> impl Iterator<Item = SyntaxNode> {
        let has_for = self.has_for();
        self.syntax
            .children_with_tokens()
            .skip_while(|e| e.kind() != SyntaxKind::ForKw)
            .skip(usize::from(has_for))
            .filter_map(rowan::NodeOrToken::into_node)
            .filter(move |_| has_for)
    }
}

impl KeyframesRule {
    /// Returns the name token (`fade` in `@keyframes fade { ... }`).
    #[must_use]
    pub fn name(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax)
            .filter(|t| t.kind() == SyntaxKind::Ident)
            .nth(1)
    }

    /// Returns the keyframe blocks, in source order.
    #[must_use]
    pub fn blocks(&self) -> AstChildren<KeyframeBlock> {
        support::children(&self.syntax)
    }
}

impl KeyframeBlock {
    /// Returns the comma-separated selectors (`from`, `50%`, `to`).
    #[must_use]
    pub fn selectors(&self) -> AstChildren<KeyframeSelector> {
        support::children(&self.syntax)
    }

    /// Returns the items of the block; valid ones are declarations.
    #[must_use]
    pub fn items(&self) -> AstChildren<CssItem> {
        support::children(&self.syntax)
    }
}

impl KeyframeSelector {
    /// Returns the selector token: an `Ident` (`from`, `to`) or a `UnitLit`
    /// (`50%`).
    #[must_use]
    pub fn token(&self) -> Option<SyntaxToken> {
        support::tokens(&self.syntax).next()
    }
}

impl Declaration {
    /// Returns the property name token (`borderLeft`).
    #[must_use]
    pub fn property(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::Ident)
    }

    /// Returns the comma-separated values.
    #[must_use]
    pub fn values(&self) -> AstChildren<CssValue> {
        support::children(&self.syntax)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rowan::GreenNodeBuilder;

    /// A tiny tree-building DSL for tests: `N(kind)` opens a node, `T(kind,
    /// text)` adds a token and `F` finishes the current node.
    enum Ev {
        N(SyntaxKind),
        T(SyntaxKind, &'static str),
        F,
    }
    use Ev::{F, N, T};
    use SyntaxKind as K;

    fn build(events: &[Ev]) -> SyntaxNode {
        let mut b = GreenNodeBuilder::new();
        for event in events {
            match event {
                N(kind) => b.start_node((*kind).into()),
                T(kind, text) => b.token((*kind).into(), text),
                F => b.finish_node(),
            }
        }
        SyntaxNode::new_root(b.finish())
    }

    #[test]
    fn every_node_kind_has_a_wrapper() {
        let wrapped: std::collections::HashSet<_> = WRAPPED_KINDS.iter().copied().collect();
        for kind in SyntaxKind::ALL.iter().copied().filter(|k| k.is_node()) {
            assert!(wrapped.contains(&kind), "{kind:?} has no typed wrapper");
        }
    }

    #[test]
    fn component_with_annotated_field_and_function() {
        // @Prop public component Counter { @State var count: Int = 0  func increment() {} }
        let root = build(&[
            N(K::SourceFile),
            N(K::ComponentDecl),
            N(K::Annotation),
            T(K::At, "@"),
            T(K::Ident, "Prop"),
            F,
            T(K::PublicKw, "public"),
            T(K::Whitespace, " "),
            T(K::ComponentKw, "component"),
            T(K::Whitespace, " "),
            T(K::Ident, "Counter"),
            N(K::MemberList),
            T(K::LBrace, "{"),
            N(K::FieldDecl),
            N(K::Annotation),
            T(K::At, "@"),
            T(K::Ident, "State"),
            F,
            T(K::VarKw, "var"),
            T(K::Ident, "count"),
            T(K::Colon, ":"),
            N(K::PathType),
            T(K::Ident, "Int"),
            F,
            T(K::Eq, "="),
            N(K::Literal),
            T(K::IntLit, "0"),
            F,
            F,
            N(K::FuncDecl),
            T(K::FuncKw, "func"),
            T(K::Ident, "increment"),
            N(K::ParamList),
            T(K::LParen, "("),
            T(K::RParen, ")"),
            F,
            N(K::Block),
            T(K::LBrace, "{"),
            T(K::RBrace, "}"),
            F,
            F,
            T(K::RBrace, "}"),
            F,
            F,
            F,
        ]);
        let file = SourceFile::cast(root).unwrap();
        let Some(Item::ComponentDecl(component)) = file.items().next() else {
            panic!()
        };
        assert_eq!(component.name_text().as_deref(), Some("Counter"));
        assert_eq!(component.visibility(), Visibility::Public);
        assert!(component.annotation("Prop").is_some());
        let members = component.members();
        assert_eq!(members.len(), 2);
        let Member::FieldDecl(field) = &members[0] else {
            panic!()
        };
        assert_eq!(field.name_text().as_deref(), Some("count"));
        assert!(field.is_var());
        assert!(!field.is_static());
        assert!(field.annotation("State").is_some());
        assert!(matches!(field.ty(), Some(Type::PathType(_))));
        let Some(Expr::Literal(lit)) = field.initializer() else {
            panic!()
        };
        assert_eq!(lit.token().unwrap().text(), "0");
        let Member::FuncDecl(func) = &members[1] else {
            panic!()
        };
        assert_eq!(func.name_text().as_deref(), Some("increment"));
        assert_eq!(func.visibility(), Visibility::Module);
        assert!(func.body().unwrap().statements().next().is_none());
        assert!(!func.is_async());
    }

    #[test]
    fn call_with_labeled_bind_argument_and_trailing_lambda() {
        // Stepper(value: bind quantity) { x }
        let root = build(&[
            N(K::CallExpr),
            N(K::NameExpr),
            T(K::Ident, "Stepper"),
            F,
            N(K::ArgList),
            T(K::LParen, "("),
            N(K::Arg),
            T(K::Ident, "value"),
            T(K::Colon, ":"),
            T(K::Whitespace, " "),
            N(K::BindExpr),
            T(K::BindKw, "bind"),
            N(K::NameExpr),
            T(K::Ident, "quantity"),
            F,
            F,
            F,
            T(K::RParen, ")"),
            F,
            N(K::LambdaExpr),
            T(K::LBrace, "{"),
            N(K::NameExpr),
            T(K::Ident, "x"),
            F,
            T(K::RBrace, "}"),
            F,
            F,
        ]);
        let call = CallExpr::cast(root).unwrap();
        assert!(matches!(call.callee(), Some(Expr::NameExpr(_))));
        let arg = call.arg_list().unwrap().args().next().unwrap();
        assert_eq!(arg.label().unwrap().text(), "value");
        assert!(matches!(arg.value(), Some(Expr::BindExpr(_))));
        let lambda = call.trailing_lambda().unwrap();
        assert!(matches!(
            lambda.statements().next(),
            Some(Stmt::Expr(Expr::NameExpr(_)))
        ));
    }

    #[test]
    fn unlabeled_argument_has_no_label() {
        let root = build(&[N(K::Arg), N(K::NameExpr), T(K::Ident, "id"), F, F]);
        let arg = Arg::cast(root).unwrap();
        assert!(arg.label().is_none());
        assert!(arg.value().is_some());
    }

    #[test]
    fn param_label_and_name() {
        let labeled = build(&[
            N(K::Param),
            T(K::ForKw, "for"),
            T(K::Whitespace, " "),
            T(K::Ident, "productId"),
            T(K::Colon, ":"),
            N(K::PathType),
            T(K::Ident, "ProductId"),
            F,
            F,
        ]);
        let p = Param::cast(labeled).unwrap();
        assert_eq!(p.label().unwrap().text(), "for");
        assert_eq!(p.name().unwrap().text(), "productId");
        let plain = build(&[
            N(K::Param),
            T(K::Ident, "id"),
            T(K::Colon, ":"),
            N(K::PathType),
            T(K::Ident, "Id"),
            F,
            F,
        ]);
        let p = Param::cast(plain).unwrap();
        assert!(p.label().is_none());
        assert_eq!(p.name().unwrap().text(), "id");
    }

    #[test]
    fn string_plain_value_decodes_escapes() {
        let root = build(&[
            N(K::StringLit),
            T(K::StringStart, "\""),
            T(K::StringText, "a"),
            T(K::StringEscape, "\\n"),
            T(K::StringEscape, "{{"),
            T(K::StringEnd, "\""),
            F,
        ]);
        assert_eq!(
            StringLit::cast(root).unwrap().plain_value().as_deref(),
            Some("a\n{")
        );
    }

    #[test]
    fn stmt_casts_expressions_and_rejects_other_nodes() {
        let expr = build(&[N(K::NameExpr), T(K::Ident, "x"), F]);
        assert!(matches!(
            Stmt::cast(expr),
            Some(Stmt::Expr(Expr::NameExpr(_)))
        ));
        let other = build(&[N(K::MemberList), F]);
        assert!(Stmt::cast(other).is_none());
        assert!(Stmt::can_cast(K::ForStmt));
        assert!(!Stmt::can_cast(K::FieldDecl));
    }

    #[test]
    fn accessors_on_empty_nodes_return_none() {
        let func = FuncDecl::cast(build(&[N(K::FuncDecl), F])).unwrap();
        assert!(func.name().is_none());
        assert!(func.param_list().is_none());
        assert!(func.body().is_none());
        let call = CallExpr::cast(build(&[N(K::CallExpr), F])).unwrap();
        assert!(call.callee().is_none());
        assert!(call.trailing_lambda().is_none());
        let binary = BinaryExpr::cast(build(&[N(K::BinaryExpr), F])).unwrap();
        assert!(binary.lhs().is_none() && binary.rhs().is_none() && binary.op().is_none());
    }
}
