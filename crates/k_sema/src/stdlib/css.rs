//! The CSS properties of style blocks (`docs/design/06-styles.md`) as data.
//!
//! Each [`CssProperty`] has its camel-case K name, the CSS name it maps to
//! 1:1, its value grammar in the CSS value-definition syntax of the
//! specifications (`<line-width> || <line-style> || <color>`), and the K
//! types that an implicit member (`.md`) or an interpolation (`{accent}`)
//! in its value resolves against. A property that isn't in the table is a
//! compile error with a "did you mean" hint.
//!
//! The value grammar is checked by the style checker
//! (`docs/compiler/sema.md`, "Styles"): CSS keywords are bare identifiers,
//! literals must match a data type of [`DATA_TYPES`], and K values must
//! have one of the property's [`CssProperty::types`].

/// A CSS data type of the value grammars and the K types whose values
/// satisfy it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DataType {
    /// The data type as written in grammars, such as `<length>`.
    pub name: &'static str,
    /// The K types (or literal kinds) that satisfy it.
    pub types: &'static [&'static str],
}

/// Every data type the grammars below use, sorted by name.
pub const DATA_TYPES: &[DataType] = &[
    DataType {
        name: "<angle>",
        types: &["Angle"],
    },
    DataType {
        name: "<color>",
        types: &["Color"],
    },
    DataType {
        name: "<custom-ident>",
        types: &[],
    },
    DataType {
        name: "<flex>",
        types: &["Length"],
    },
    DataType {
        name: "<font>",
        types: &["Font"],
    },
    DataType {
        name: "<integer>",
        types: &["Int"],
    },
    DataType {
        name: "<keyframes-name>",
        // The component's generated keyframes type (see `sema.md`, "Styles").
        types: &["Keyframes"],
    },
    DataType {
        name: "<length-percentage>",
        types: &["Length", "Spacing", "Radius", "FontSize", "Percent"],
    },
    DataType {
        name: "<length>",
        types: &["Length", "Spacing", "Radius", "FontSize"],
    },
    DataType {
        name: "<line-style>",
        types: &[],
    },
    DataType {
        name: "<line-width>",
        types: &["Length"],
    },
    DataType {
        name: "<number>",
        types: &["Double", "Int"],
    },
    DataType {
        name: "<percentage>",
        types: &["Percent"],
    },
    DataType {
        name: "<string>",
        types: &["String"],
    },
    DataType {
        name: "<time>",
        types: &["Duration"],
    },
    DataType {
        name: "<url>",
        types: &["Url"],
    },
];

/// One CSS property.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CssProperty {
    /// The K name, in camel case (`borderLeft`).
    pub name: &'static str,
    /// The CSS name (`border-left`).
    pub css: &'static str,
    /// The value grammar, in CSS value-definition syntax.
    pub syntax: &'static str,
    /// The K types implicit members and interpolations in the value resolve
    /// against, in the order autocomplete lists them. `padding: .md` finds
    /// `Spacing.md`.
    pub types: &'static [&'static str],
}

/// Defines a property. `css` is always [`kebab_case`] of `name`, which the
/// tests check; it is spelled out so the table reads like the CSS it maps to.
const fn property(
    name: &'static str,
    css: &'static str,
    syntax: &'static str,
    types: &'static [&'static str],
) -> CssProperty {
    CssProperty {
        name,
        css,
        syntax,
        types,
    }
}

/// The value types of spacing-like properties (`padding`, `gap`, the
/// insets `top`/`left`/…): spacing tokens, lengths and percentages.
const SPACING: &[&str] = &["Spacing", "Length", "Percent"];
/// The value types of color properties.
const COLOR: &[&str] = &["Color"];
/// The value types of length properties: lengths and percentages.
const LENGTH: &[&str] = &["Length", "Percent"];
/// For properties whose values are keywords only (no K type).
const NONE: &[&str] = &[];

/// Every supported property, sorted by K name.
pub const PROPERTIES: &[CssProperty] = &[
    property(
        "alignContent",
        "align-content",
        "normal | stretch | center | start | end | flex-start | flex-end | space-between | space-around | space-evenly",
        NONE,
    ),
    property(
        "alignItems",
        "align-items",
        "normal | stretch | center | start | end | flex-start | flex-end | baseline",
        NONE,
    ),
    property(
        "alignSelf",
        "align-self",
        "auto | normal | stretch | center | start | end | flex-start | flex-end | baseline",
        NONE,
    ),
    property(
        "animationDelay",
        "animation-delay",
        "<time>#",
        &["Duration"],
    ),
    property(
        "animationDirection",
        "animation-direction",
        "[ normal | reverse | alternate | alternate-reverse ]#",
        NONE,
    ),
    property(
        "animationDuration",
        "animation-duration",
        "<time>#",
        &["Duration"],
    ),
    property(
        "animationFillMode",
        "animation-fill-mode",
        "[ none | forwards | backwards | both ]#",
        NONE,
    ),
    property(
        "animationIterationCount",
        "animation-iteration-count",
        "[ infinite | <number> ]#",
        NONE,
    ),
    property(
        "animationName",
        "animation-name",
        "[ none | <keyframes-name> ]#",
        &["Keyframes"],
    ),
    property(
        "animationTimingFunction",
        "animation-timing-function",
        "[ linear | ease | ease-in | ease-out | ease-in-out | step-start | step-end ]#",
        NONE,
    ),
    property(
        "aspectRatio",
        "aspect-ratio",
        "auto | <number> [ / <number> ]?",
        NONE,
    ),
    property("backgroundColor", "background-color", "<color>", COLOR),
    property(
        "backgroundImage",
        "background-image",
        "none | <url>",
        &["Url"],
    ),
    property(
        "border",
        "border",
        "<line-width> || <line-style> || <color>",
        &["Color", "Length"],
    ),
    property(
        "borderBottom",
        "border-bottom",
        "<line-width> || <line-style> || <color>",
        &["Color", "Length"],
    ),
    property(
        "borderCollapse",
        "border-collapse",
        "collapse | separate",
        NONE,
    ),
    property("borderColor", "border-color", "<color>{1,4}", COLOR),
    property(
        "borderLeft",
        "border-left",
        "<line-width> || <line-style> || <color>",
        &["Color", "Length"],
    ),
    property(
        "borderRadius",
        "border-radius",
        "<length-percentage>{1,4}",
        &["Radius", "Length", "Percent"],
    ),
    property(
        "borderRight",
        "border-right",
        "<line-width> || <line-style> || <color>",
        &["Color", "Length"],
    ),
    property("borderStyle", "border-style", "<line-style>{1,4}", NONE),
    property(
        "borderTop",
        "border-top",
        "<line-width> || <line-style> || <color>",
        &["Color", "Length"],
    ),
    property(
        "borderWidth",
        "border-width",
        "<line-width>{1,4}",
        &["Length"],
    ),
    property("bottom", "bottom", "auto | <length-percentage>", SPACING),
    property(
        "boxShadow",
        "box-shadow",
        "none | [ inset? && <length>{2,4} && <color>? ]#",
        &["Color", "Length"],
    ),
    property("color", "color", "<color>", COLOR),
    property(
        "columnGap",
        "column-gap",
        "normal | <length-percentage>",
        SPACING,
    ),
    property(
        "cursor",
        "cursor",
        "auto | default | pointer | text | move | not-allowed | wait | progress | grab | grabbing",
        NONE,
    ),
    property(
        "display",
        "display",
        "none | block | inline | inline-block | flex | inline-flex | grid | inline-grid | contents",
        NONE,
    ),
    property(
        "flex",
        "flex",
        "none | [ <number> <number>? || <length-percentage> | auto ]",
        LENGTH,
    ),
    property(
        "flexBasis",
        "flex-basis",
        "auto | content | <length-percentage>",
        LENGTH,
    ),
    property(
        "flexDirection",
        "flex-direction",
        "row | row-reverse | column | column-reverse",
        NONE,
    ),
    property("flexGrow", "flex-grow", "<number>", NONE),
    property("flexShrink", "flex-shrink", "<number>", NONE),
    property(
        "flexWrap",
        "flex-wrap",
        "nowrap | wrap | wrap-reverse",
        NONE,
    ),
    property("font", "font", "<font>", &["Font"]),
    property(
        "fontFamily",
        "font-family",
        "[ <string> | serif | sans-serif | monospace | system-ui ]#",
        NONE,
    ),
    property(
        "fontSize",
        "font-size",
        "<length-percentage>",
        &["FontSize", "Length", "Percent"],
    ),
    property("fontStyle", "font-style", "normal | italic", NONE),
    property(
        "fontWeight",
        "font-weight",
        "normal | bold | <number>",
        NONE,
    ),
    property("gap", "gap", "<length-percentage>{1,2}", SPACING),
    property(
        "gridColumn",
        "grid-column",
        "auto | <integer> [ / <integer> ]? | span <integer>",
        NONE,
    ),
    property(
        "gridTemplateColumns",
        "grid-template-columns",
        "none | [ <length-percentage> | <flex> | auto | min-content | max-content | minmax( <length-percentage> , [ <length-percentage> | <flex> ] ) | repeat( [ <integer> | auto-fill | auto-fit ] , [ <length-percentage> | <flex> | auto | minmax( <length-percentage> , [ <length-percentage> | <flex> ] ) ]+ ) ]+",
        LENGTH,
    ),
    property(
        "gridTemplateRows",
        "grid-template-rows",
        "none | [ <length-percentage> | <flex> | auto | min-content | max-content ]+",
        LENGTH,
    ),
    property(
        "height",
        "height",
        "auto | <length-percentage> | min-content | max-content | fit-content",
        LENGTH,
    ),
    property(
        "inset",
        "inset",
        "[ auto | <length-percentage> ]{1,4}",
        SPACING,
    ),
    property(
        "justifyContent",
        "justify-content",
        "normal | center | start | end | flex-start | flex-end | space-between | space-around | space-evenly",
        NONE,
    ),
    property(
        "justifyItems",
        "justify-items",
        "normal | stretch | center | start | end | baseline",
        NONE,
    ),
    property(
        "justifySelf",
        "justify-self",
        "auto | normal | stretch | center | start | end | baseline",
        NONE,
    ),
    property("left", "left", "auto | <length-percentage>", SPACING),
    property(
        "letterSpacing",
        "letter-spacing",
        "normal | <length>",
        &["Length"],
    ),
    property(
        "lineHeight",
        "line-height",
        "normal | <number> | <length-percentage>",
        LENGTH,
    ),
    property(
        "listStyle",
        "list-style",
        "none | disc | circle | square | decimal",
        NONE,
    ),
    property(
        "margin",
        "margin",
        "[ auto | <length-percentage> ]{1,4}",
        SPACING,
    ),
    property(
        "marginBottom",
        "margin-bottom",
        "auto | <length-percentage>",
        SPACING,
    ),
    property(
        "marginLeft",
        "margin-left",
        "auto | <length-percentage>",
        SPACING,
    ),
    property(
        "marginRight",
        "margin-right",
        "auto | <length-percentage>",
        SPACING,
    ),
    property(
        "marginTop",
        "margin-top",
        "auto | <length-percentage>",
        SPACING,
    ),
    property(
        "maxHeight",
        "max-height",
        "none | <length-percentage>",
        LENGTH,
    ),
    property(
        "maxWidth",
        "max-width",
        "none | <length-percentage>",
        LENGTH,
    ),
    property(
        "minHeight",
        "min-height",
        "auto | <length-percentage>",
        LENGTH,
    ),
    property(
        "minWidth",
        "min-width",
        "auto | <length-percentage>",
        LENGTH,
    ),
    property(
        "objectFit",
        "object-fit",
        "fill | contain | cover | none | scale-down",
        NONE,
    ),
    property(
        "opacity",
        "opacity",
        "<number> | <percentage>",
        &["Percent"],
    ),
    property(
        "outline",
        "outline",
        "<line-width> || <line-style> || <color>",
        &["Color", "Length"],
    ),
    property("outlineOffset", "outline-offset", "<length>", &["Length"]),
    property(
        "overflow",
        "overflow",
        "[ visible | hidden | clip | scroll | auto ]{1,2}",
        NONE,
    ),
    property("padding", "padding", "<length-percentage>{1,4}", SPACING),
    property(
        "paddingBottom",
        "padding-bottom",
        "<length-percentage>",
        SPACING,
    ),
    property(
        "paddingLeft",
        "padding-left",
        "<length-percentage>",
        SPACING,
    ),
    property(
        "paddingRight",
        "padding-right",
        "<length-percentage>",
        SPACING,
    ),
    property("paddingTop", "padding-top", "<length-percentage>", SPACING),
    property("pointerEvents", "pointer-events", "auto | none", NONE),
    property(
        "position",
        "position",
        "static | relative | absolute | fixed | sticky",
        NONE,
    ),
    property("right", "right", "auto | <length-percentage>", SPACING),
    property("rotate", "rotate", "none | <angle>", &["Angle"]),
    property("rowGap", "row-gap", "normal | <length-percentage>", SPACING),
    property(
        "textAlign",
        "text-align",
        "start | end | left | right | center | justify",
        NONE,
    ),
    property(
        "textDecoration",
        "text-decoration",
        "none | underline | line-through | overline",
        NONE,
    ),
    property("textOverflow", "text-overflow", "clip | ellipsis", NONE),
    property(
        "textTransform",
        "text-transform",
        "none | capitalize | uppercase | lowercase",
        NONE,
    ),
    property("top", "top", "auto | <length-percentage>", SPACING),
    property(
        "transform",
        "transform",
        "none | [ translate( <length-percentage> , <length-percentage> ) | rotate( <angle> ) | scale( <number> ) ]+",
        &["Length", "Angle"],
    ),
    property(
        "transition",
        "transition",
        "[ none | all | <custom-ident> ] || <time> || [ linear | ease | ease-in | ease-out | ease-in-out ] || <time>",
        &["Duration"],
    ),
    property(
        "verticalAlign",
        "vertical-align",
        "baseline | top | middle | bottom | text-top | text-bottom",
        NONE,
    ),
    property(
        "visibility",
        "visibility",
        "visible | hidden | collapse",
        NONE,
    ),
    property(
        "whiteSpace",
        "white-space",
        "normal | nowrap | pre | pre-wrap | pre-line",
        NONE,
    ),
    property(
        "width",
        "width",
        "auto | <length-percentage> | min-content | max-content | fit-content",
        LENGTH,
    ),
    property("zIndex", "z-index", "auto | <integer>", NONE),
];

/// The media features `@media (feature: value)` accepts, as K names with
/// their CSS names. Range syntax is deferred (grammar RA-28).
pub const MEDIA_FEATURES: &[(&str, &str)] = &[
    ("maxHeight", "max-height"),
    ("maxWidth", "max-width"),
    ("minHeight", "min-height"),
    ("minWidth", "min-width"),
    ("orientation", "orientation"),
    ("prefersColorScheme", "prefers-color-scheme"),
    ("prefersContrast", "prefers-contrast"),
    ("prefersReducedMotion", "prefers-reduced-motion"),
];

/// The at-rules a style block accepts besides `@keyframes`.
pub const AT_RULES: &[&str] = &["container", "layer", "media"];

/// Returns the property with the K name `name`.
#[must_use]
pub fn property_named(name: &str) -> Option<&'static CssProperty> {
    PROPERTIES
        .binary_search_by(|p| p.name.cmp(name))
        .ok()
        .map(|i| &PROPERTIES[i])
}

/// Returns the data type named `name` (with its angle brackets).
#[must_use]
pub fn data_type(name: &str) -> Option<&'static DataType> {
    DATA_TYPES
        .binary_search_by(|d| d.name.cmp(name))
        .ok()
        .map(|i| &DATA_TYPES[i])
}

/// Converts a camel-case K property name to its kebab-case CSS name
/// (`borderLeft` → `border-left`), the mapping every table entry follows.
#[must_use]
pub fn kebab_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
