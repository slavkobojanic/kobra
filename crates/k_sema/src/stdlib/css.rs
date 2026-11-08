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
//!
//! **Keywords are camel case**, like property names (`docs/design/06-styles.md`,
//! "Keyword values"): the grammars say `spaceBetween` and `lineThrough`, and
//! code generation prints [`keyword_css`] of each (`space-between`). A
//! kebab-case keyword in a source is K3214 with the [`camel_case`] fix.
//!
//! # API for editors
//!
//! Completion and hover read the tables through these functions, which
//! need no compilation:
//!
//! * [`property_names`]: every property's K name, sorted; [`property_named`]
//!   gives a property's CSS name, grammar and K types (the types are what
//!   implicit members such as `.md` complete against).
//! * [`keywords`]: the camel-case keywords a property's value accepts, in
//!   grammar order, including those of its data types (`<line-style>`,
//!   `<line-width>`, named `<color>`s); [`WIDE_KEYWORDS`] are accepted by
//!   every property alone and are not repeated there.
//! * [`MEDIA_FEATURES`] and [`media_feature_keywords`]: the features of
//!   `@media ( … )` and the keywords their values take.

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
        "normal | stretch | center | start | end | flexStart | flexEnd | spaceBetween | spaceAround | spaceEvenly",
        NONE,
    ),
    property(
        "alignItems",
        "align-items",
        "normal | stretch | center | start | end | flexStart | flexEnd | baseline",
        NONE,
    ),
    property(
        "alignSelf",
        "align-self",
        "auto | normal | stretch | center | start | end | flexStart | flexEnd | baseline",
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
        "[ normal | reverse | alternate | alternateReverse ]#",
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
        "[ linear | ease | easeIn | easeOut | easeInOut | stepStart | stepEnd ]#",
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
        "auto | default | pointer | text | move | notAllowed | wait | progress | grab | grabbing",
        NONE,
    ),
    property(
        "display",
        "display",
        "none | block | inline | inlineBlock | flex | inlineFlex | grid | inlineGrid | contents",
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
        "row | rowReverse | column | columnReverse",
        NONE,
    ),
    property("flexGrow", "flex-grow", "<number>", NONE),
    property("flexShrink", "flex-shrink", "<number>", NONE),
    property("flexWrap", "flex-wrap", "nowrap | wrap | wrapReverse", NONE),
    property("font", "font", "<font>", &["Font"]),
    property(
        "fontFamily",
        "font-family",
        "[ <string> | serif | sansSerif | monospace | systemUi ]#",
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
        "none | [ <length-percentage> | <flex> | auto | minContent | maxContent | minmax( <length-percentage> , [ <length-percentage> | <flex> ] ) | repeat( [ <integer> | autoFill | autoFit ] , [ <length-percentage> | <flex> | auto | minmax( <length-percentage> , [ <length-percentage> | <flex> ] ) ]+ ) ]+",
        LENGTH,
    ),
    property(
        "gridTemplateRows",
        "grid-template-rows",
        "none | [ <length-percentage> | <flex> | auto | minContent | maxContent ]+",
        LENGTH,
    ),
    property(
        "height",
        "height",
        "auto | <length-percentage> | minContent | maxContent | fitContent",
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
        "normal | center | start | end | flexStart | flexEnd | spaceBetween | spaceAround | spaceEvenly",
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
        "fill | contain | cover | none | scaleDown",
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
        "none | underline | lineThrough | overline",
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
        "[ none | all | <custom-ident> ] || <time> || [ linear | ease | easeIn | easeOut | easeInOut ] || <time>",
        &["Duration"],
    ),
    property(
        "verticalAlign",
        "vertical-align",
        "baseline | top | middle | bottom | textTop | textBottom",
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
        "normal | nowrap | pre | preWrap | preLine",
        NONE,
    ),
    property(
        "width",
        "width",
        "auto | <length-percentage> | minContent | maxContent | fitContent",
        LENGTH,
    ),
    property("zIndex", "z-index", "auto | <integer>", NONE),
];

/// The CSS-wide keywords every property accepts as its whole value.
pub const WIDE_KEYWORDS: &[&str] = &["inherit", "initial", "revert", "unset"];

/// The named colors `<color>` accepts besides literals and `Color` values.
/// `currentColor` is spelled as in CSS, which matches it case-insensitively.
pub const NAMED_COLORS: &[&str] = &[
    "aqua",
    "black",
    "blue",
    "currentColor",
    "fuchsia",
    "gray",
    "green",
    "grey",
    "lime",
    "maroon",
    "navy",
    "olive",
    "orange",
    "purple",
    "red",
    "silver",
    "teal",
    "transparent",
    "white",
    "yellow",
];

/// The keywords of the `<line-style>` data type.
pub const LINE_STYLES: &[&str] = &[
    "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset",
];

/// The keywords of the `<line-width>` data type.
pub const LINE_WIDTHS: &[&str] = &["thin", "medium", "thick"];

/// The keywords data type `name` (with its angle brackets) accepts, such as
/// [`LINE_STYLES`] for `<line-style>`; empty for most data types.
#[must_use]
pub fn data_type_keywords(name: &str) -> &'static [&'static str] {
    match name {
        "<line-style>" => LINE_STYLES,
        "<line-width>" => LINE_WIDTHS,
        "<color>" => NAMED_COLORS,
        _ => &[],
    }
}

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

/// The keywords the value of media feature `feature` (a K name of
/// [`MEDIA_FEATURES`]) takes, in camel case; empty for the range features
/// (`maxWidth`), whose values are lengths, and for unknown names.
#[must_use]
pub fn media_feature_keywords(feature: &str) -> &'static [&'static str] {
    match feature {
        "orientation" => &["portrait", "landscape"],
        "prefersColorScheme" => &["light", "dark"],
        "prefersContrast" => &["noPreference", "more", "less", "custom"],
        "prefersReducedMotion" => &["noPreference", "reduce"],
        _ => &[],
    }
}

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

/// The K names of every supported property, sorted: what a completion
/// engine offers at a declaration's name.
pub fn property_names() -> impl Iterator<Item = &'static str> {
    PROPERTIES.iter().map(|p| p.name)
}

/// The camel-case keywords the value of property `name` (a K name such as
/// `justifyContent`) accepts, in the order its grammar lists them, without
/// duplicates: the grammar's own keywords (`spaceBetween`) followed, at
/// the place of each data type, by that type's keywords
/// ([`data_type_keywords`]: `solid`, `thick`, `currentColor`, …). Function
/// names (`repeat`, `minmax`) and data types are not keywords.
/// [`WIDE_KEYWORDS`] are left out. An unknown property has none.
///
/// This is what a completion engine offers in a declaration's value, next
/// to the implicit members of the property's [`CssProperty::types`].
#[must_use]
pub fn keywords(name: &str) -> Vec<&'static str> {
    let Some(prop) = property_named(name) else {
        return Vec::new();
    };
    let mut out: Vec<&'static str> = Vec::new();
    let syntax = prop.syntax;
    let bytes = syntax.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'<' {
            let end = syntax[i..].find('>').map_or(bytes.len(), |e| i + e + 1);
            for k in data_type_keywords(&syntax[i..end]) {
                if !out.contains(k) {
                    out.push(k);
                }
            }
            i = end;
        } else if b == b'{' {
            i = syntax[i..].find('}').map_or(bytes.len(), |e| i + e + 1);
        } else if b.is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                i += 1;
            }
            let word = &syntax[start..i];
            if bytes.get(i) != Some(&b'(') && !out.contains(&word) {
                out.push(word);
            }
        } else {
            i += 1;
        }
    }
    out
}

/// The CSS spelling of a camel-case keyword: [`kebab_case`]
/// (`lineThrough` → `line-through`, `backgroundColor` → `background-color`
/// in `transition`), except for `currentColor`, which CSS spells that way.
/// A word that doesn't start with a lower-case letter is returned as is.
#[must_use]
pub fn keyword_css(word: &str) -> std::borrow::Cow<'_, str> {
    if word == "currentColor"
        || !word.starts_with(|c: char| c.is_ascii_lowercase())
        || !word.contains(|c: char| c.is_ascii_uppercase())
    {
        return std::borrow::Cow::Borrowed(word);
    }
    std::borrow::Cow::Owned(kebab_case(word))
}

/// The camel-case K spelling of a kebab-case CSS word (`line-through` →
/// `lineThrough`), the fix of K3214.
#[must_use]
pub fn camel_case(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut upper = false;
    for c in word.chars() {
        if c == '-' {
            upper = !out.is_empty();
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}
