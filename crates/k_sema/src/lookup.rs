//! Results of the position-based queries of an [`Analysis`](crate::Analysis):
//! what a name refers to, and which side of the server/client boundary a
//! declaration is on.
//!
//! Their text forms are the formats of the `sema` corpus suite's caret
//! annotations (`docs/compiler/testing.md`, "Annotations"), so a test can
//! write `// ^^^^^ resolves: member shop.cart.CartService.add(product:)`.

use std::fmt;

use k_diagnostics::{LineCol, Span};

/// What a name, member, element, property or class reference at a span
/// refers to.
///
/// Qualified names follow one rule: a declaration of the prelude is written
/// by its simple name (`Int`, `Page`, `Color`), every other declaration by
/// its fully qualified name (`shop.cart.CartItem`, `k.http.HttpStatus`).
/// Functions carry their argument labels, Swift-style (`add(product:)`,
/// `find(_:)`, `refresh()`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Resolution {
    /// A top-level declaration: `decl shop.cart.CartService`.
    Declaration {
        /// The qualified name.
        name: String,
    },
    /// A member of a type: a field, function, computed property, enum case
    /// or generated member (`member Page.product`,
    /// `member shop.cart.CartService.add(product:)`).
    Member {
        /// The qualified name of the type that declares the member.
        owner: String,
        /// The member's name, with labels for functions.
        name: String,
    },
    /// A local binding (`let`, a pattern binding, a lambda parameter, a `for`
    /// variable): `local items 3:9`, with the position of its binding name.
    Local {
        /// The bound name.
        name: String,
        /// Where the name is bound.
        definition: Span,
    },
    /// A function parameter: `param id 2:14`.
    Parameter {
        /// The parameter's internal name.
        name: String,
        /// Where the parameter is declared.
        definition: Span,
    },
    /// A built-in element: `element button`.
    Element {
        /// The element's K name.
        name: String,
    },
    /// An attribute of a built-in element: `attribute button.type`.
    Attribute {
        /// The element's K name.
        element: String,
        /// The attribute's label.
        name: String,
    },
    /// An event in `.on(name: …)`: `event button.click` or
    /// `event shop.Counter.changed`.
    Event {
        /// The element's name or the component's qualified name.
        owner: String,
        /// The event's name.
        name: String,
    },
    /// A CSS property in a style block: `css padding`.
    CssProperty {
        /// The K property name.
        name: String,
    },
    /// A class of a component's style block: `class shop.Summary.muted`.
    Class {
        /// The qualified name of the component.
        component: String,
        /// The class name.
        name: String,
    },
    /// Keyframes of a component's style block: `keyframes shop.Toast.fade`.
    Keyframes {
        /// The qualified name of the component.
        component: String,
        /// The keyframes name.
        name: String,
    },
    /// A theme token, a static member of its category type:
    /// `token Spacing.md`.
    Token {
        /// The category type, such as `Spacing` or `Color`.
        category: String,
        /// The token name.
        name: String,
    },
}

impl Resolution {
    /// Renders the resolution in the corpus annotation format. `position`
    /// converts a span to the 1-based position of its start; it is called
    /// only for locals and parameters, and `?:?` is written when it returns
    /// `None`.
    pub fn render(&self, position: impl Fn(Span) -> Option<LineCol>) -> String {
        let at = |span: Span| position(span).map_or_else(|| "?:?".to_owned(), |p| p.to_string());
        match self {
            Resolution::Declaration { name } => format!("decl {name}"),
            Resolution::Member { owner, name } => format!("member {owner}.{name}"),
            Resolution::Local { name, definition } => format!("local {name} {}", at(*definition)),
            Resolution::Parameter { name, definition } => {
                format!("param {name} {}", at(*definition))
            }
            Resolution::Element { name } => format!("element {name}"),
            Resolution::Attribute { element, name } => format!("attribute {element}.{name}"),
            Resolution::Event { owner, name } => format!("event {owner}.{name}"),
            Resolution::CssProperty { name } => format!("css {name}"),
            Resolution::Class { component, name } => format!("class {component}.{name}"),
            Resolution::Keyframes { component, name } => format!("keyframes {component}.{name}"),
            Resolution::Token { category, name } => format!("token {category}.{name}"),
        }
    }
}

/// Which side of the server/client boundary a declaration is on
/// (`docs/design/10-services.md`, "Server/client boundary").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Side {
    /// Only in the server bundle.
    ServerOnly,
    /// Only in the client bundle.
    ClientOnly,
    /// Usable on both sides.
    Shared,
}

impl fmt::Display for Side {
    /// Writes `server-only`, `client-only` or `shared`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Side::ServerOnly => "server-only",
            Side::ClientOnly => "client-only",
            Side::Shared => "shared",
        })
    }
}

/// A query was abandoned because the inputs changed while it ran
/// ([`AnalysisHost::apply_change`](crate::AnalysisHost::apply_change)).
/// Retry on a new [`Analysis`](crate::Analysis).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the analysis was cancelled because its inputs changed")
    }
}

impl std::error::Error for Cancelled {}

#[cfg(test)]
mod tests {
    use super::*;
    use k_diagnostics::{FileId, TextRange};

    #[test]
    fn resolutions_render_in_the_annotation_format() {
        let span = Span::new(FileId::new(3), TextRange::empty(0.into()));
        let at = |_| Some(LineCol { line: 3, col: 9 });
        let cases = [
            (
                Resolution::Declaration {
                    name: "shop.Cart".into(),
                },
                "decl shop.Cart",
            ),
            (
                Resolution::Member {
                    owner: "Page".into(),
                    name: "product".into(),
                },
                "member Page.product",
            ),
            (
                Resolution::Local {
                    name: "items".into(),
                    definition: span,
                },
                "local items 3:9",
            ),
            (
                Resolution::Parameter {
                    name: "id".into(),
                    definition: span,
                },
                "param id 3:9",
            ),
            (
                Resolution::Element {
                    name: "button".into(),
                },
                "element button",
            ),
            (
                Resolution::Attribute {
                    element: "button".into(),
                    name: "type".into(),
                },
                "attribute button.type",
            ),
            (
                Resolution::Event {
                    owner: "button".into(),
                    name: "click".into(),
                },
                "event button.click",
            ),
            (
                Resolution::CssProperty {
                    name: "padding".into(),
                },
                "css padding",
            ),
            (
                Resolution::Class {
                    component: "corpus.Summary".into(),
                    name: "muted".into(),
                },
                "class corpus.Summary.muted",
            ),
            (
                Resolution::Keyframes {
                    component: "corpus.Toast".into(),
                    name: "fade".into(),
                },
                "keyframes corpus.Toast.fade",
            ),
            (
                Resolution::Token {
                    category: "Spacing".into(),
                    name: "md".into(),
                },
                "token Spacing.md",
            ),
        ];
        for (resolution, text) in cases {
            assert_eq!(resolution.render(at), text);
        }
        let unknown = Resolution::Local {
            name: "x".into(),
            definition: span,
        };
        assert_eq!(unknown.render(|_| None), "local x ?:?");
        assert_eq!(Side::ServerOnly.to_string(), "server-only");
        assert_eq!(Side::ClientOnly.to_string(), "client-only");
        assert_eq!(Side::Shared.to_string(), "shared");
    }
}
