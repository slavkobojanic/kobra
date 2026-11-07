//! The lossless syntax tree: rowan language binding and the parse result.
//!
//! K uses the green/red tree design of [rowan]: an immutable, deduplicated
//! *green* tree stores kinds and text (and can be shared between versions of
//! a file for incremental reparsing), and a lazily built *red* tree adds
//! parent pointers and absolute offsets on demand. Typed accessors live in
//! [`crate::ast`].

use k_diagnostics::Diagnostic;
use rowan::GreenNode;

use crate::ast::{self, AstNode};
use crate::kind::SyntaxKind;

/// The rowan language tag for K syntax trees.
///
/// This type is uninhabited; it exists only to parameterize rowan's generic
/// tree types.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum KLanguage {}

impl rowan::Language for KLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// A node of the red tree.
pub type SyntaxNode = rowan::SyntaxNode<KLanguage>;
/// A token of the red tree.
pub type SyntaxToken = rowan::SyntaxToken<KLanguage>;
/// Either a node or a token of the red tree.
pub type SyntaxElement = rowan::SyntaxElement<KLanguage>;
/// An iterator over the child nodes of a node.
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<KLanguage>;

/// The result of parsing one source file: a complete syntax tree plus the
/// diagnostics of the lexer and the parser.
///
/// The tree is always complete and lossless, even for invalid input:
/// `parse.syntax().text()` equals the source text.
///
/// # Deep trees
///
/// rowan frees a tree recursively, one native stack frame chain per level,
/// so dropping a very deep tree (a 10,000-term `a + b + ...` chain nests
/// 10,000 `BinaryExpr` nodes) could overflow the stack. A `Parse` whose tree
/// is deeper than [`Parse::SHALLOW_DEPTH`] therefore tears its tree down
/// iteratively when dropped. Red nodes obtained from [`Parse::syntax`] keep
/// the tree alive; drop them before the `Parse` to benefit from this.
#[derive(Clone, Debug)]
pub struct Parse {
    /// The root of the lossless green tree, a `SourceFile` node.
    green: GreenNode,
    /// Lexer and parser diagnostics, in the order given to the constructor
    /// (source order when built by the parser).
    diagnostics: Vec<Diagnostic>,
    /// The nesting depth of the tree (the root has depth 1).
    depth: u32,
}

impl Parse {
    /// Trees at most this deep are dropped by rowan directly.
    pub const SHALLOW_DEPTH: u32 = 256;

    /// Creates a parse result from a green tree and its diagnostics.
    ///
    /// The depth of the tree is not known here, so dropping the result
    /// always uses the iterative teardown; the parser uses an internal
    /// constructor that records the depth.
    #[must_use]
    pub fn new(green: GreenNode, diagnostics: Vec<Diagnostic>) -> Parse {
        Parse {
            green,
            diagnostics,
            depth: u32::MAX,
        }
    }

    /// Creates a parse result whose tree is known to be `depth` levels deep.
    pub(crate) fn with_depth(green: GreenNode, diagnostics: Vec<Diagnostic>, depth: u32) -> Parse {
        Parse {
            green,
            diagnostics,
            depth,
        }
    }

    /// Returns the nesting depth of the tree; the root alone has depth 1.
    /// Returns `u32::MAX` for results built with [`Parse::new`].
    #[must_use]
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Returns the green (immutable, shareable) root of the tree.
    #[must_use]
    pub fn green(&self) -> &GreenNode {
        &self.green
    }

    /// Builds the red root node, whose kind is [`SyntaxKind::SourceFile`].
    #[must_use]
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// Returns the typed root of the tree.
    ///
    /// Returns `None` only if the root is not a `SourceFile`, which the
    /// parser never produces.
    #[must_use]
    pub fn tree(&self) -> Option<ast::SourceFile> {
        ast::SourceFile::cast(self.syntax())
    }

    /// Returns the lexer and parser diagnostics, in source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl Drop for Parse {
    fn drop(&mut self) {
        if self.depth <= Parse::SHALLOW_DEPTH {
            return;
        }
        // Hold a reference to every node in breadth-first order (parents
        // before children), then release them in that order: each node is
        // freed while its children are still referenced by the list, so no
        // free ever recurses into the next level.
        let placeholder = GreenNode::new(SyntaxKind::SourceFile.into(), std::iter::empty());
        let root = std::mem::replace(&mut self.green, placeholder);
        let mut nodes: Vec<GreenNode> = vec![root.clone()];
        let mut index = 0;
        while index < nodes.len() {
            // A clone (one reference count) lets `nodes` grow while the
            // children are read; collecting them into a temporary `Vec`
            // instead would allocate once per node.
            let node = nodes[index].clone();
            nodes.extend(
                node.children()
                    .filter_map(rowan::NodeOrToken::into_node)
                    .map(ToOwned::to_owned),
            );
            index += 1;
        }
        drop(root);
        for node in nodes {
            drop(node);
        }
    }
}
