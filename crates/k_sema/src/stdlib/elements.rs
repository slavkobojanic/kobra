//! The built-in view elements (`docs/design/05-views.md`) as data.
//!
//! A lowercase name called in a view is a built-in element. Each
//! [`ElementDef`] lists:
//!
//! * the **required named arguments**, in one or more alternative forms
//!   (`link(to:)` or `link(url:)`);
//! * the **attributes**, chained like optional props and generated per
//!   element (`.label`, `.type`, `.disabled`, …); they are not registered
//!   modifiers and reserve no names;
//! * the **events** `.on(name: handler)` accepts;
//! * what **content** the trailing block may hold.
//!
//! Types are written in K syntax and resolved against the prelude and the
//! listed stdlib modules ([`TYPE_SCOPE`]); `T` is a type parameter inferred
//! at each use. Chaining an attribute the element doesn't define is a
//! compile error, and an HTML element name is a compile error with the
//! hint in [`HTML_NAMES`].
//!
//! [`UNIVERSAL_ATTRIBUTES`] apply to every element. Modifiers registered by
//! libraries (`.tooltip()`, `.animate()`) are declared by those libraries,
//! not here.

/// The stdlib declarations element types may name besides the prelude.
pub const TYPE_SCOPE: &[&str] = &[
    "k.ui.AriaRole",
    "k.ui.ButtonType",
    "k.ui.ElementSize",
    "k.ui.ImageLoading",
];

/// A named argument or chained attribute of an element.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Param {
    /// The label: `description` in `image(source:, description:)`, or the
    /// chained method name for an attribute (`.placeholder`).
    pub label: &'static str,
    /// The type, in K syntax (`Binding<String>`, `Bool`, `ButtonType`).
    pub ty: &'static str,
    /// What the argument means.
    pub doc: &'static str,
}

/// An event an element fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Event {
    /// The label in `.on(name: handler)`.
    pub name: &'static str,
    /// The payload type; `Void` for none, so the handler takes no
    /// parameter.
    pub payload: &'static str,
}

/// What an element's trailing block may contain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Content {
    /// No trailing block (`lineBreak`, `image`, inputs).
    None,
    /// Any view content.
    Any,
    /// Only the listed child elements (and `if`, `for`, `match` around
    /// them).
    Only(&'static [&'static str]),
}

/// How an element behaves for accessibility rules
/// (`docs/design/05-views.md`, "Attributes are per element").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    /// Plain content: `box`, `text`, `paragraph`, …
    Content,
    /// A landmark or group whose `.label` is its accessible name.
    Landmark,
    /// A form control whose `.label` is a visible `<label>`.
    FormControl,
    /// An interactive element named by its content (`button`, `link`).
    Interactive,
    /// Media and embedded content.
    Media,
}

/// One built-in element.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ElementDef {
    /// The K name (`paragraph`).
    pub name: &'static str,
    /// The HTML element it compiles to (`p`).
    pub html: &'static str,
    /// The alternative sets of required named arguments; empty when none.
    pub forms: &'static [&'static [Param]],
    /// Chained attributes specific to the element.
    pub attributes: &'static [Param],
    /// Events `.on(…)` accepts.
    pub events: &'static [Event],
    /// What the trailing block may contain.
    pub content: Content,
    /// The accessibility category.
    pub category: Category,
}

/// Attributes every element has: `.class(...)` (typed class references,
/// set once), `.hidden(Bool)`, and the options of the `appeared` and
/// `disappeared` events (`docs/design/45-web-platform.md`, "Element
/// events").
pub const UNIVERSAL_ATTRIBUTES: &[Param] = &[
    Param {
        label: "class",
        ty: "Classes",
        doc: "class references from the component's style block; a list, with `when` for conditional classes",
    },
    Param {
        label: "hidden",
        ty: "Bool",
        doc: "hides the element from everyone, including assistive technology",
    },
    Param {
        label: "appearThreshold",
        ty: "Percent",
        doc: "how much of the element must be visible for `appeared`; `0%` (any pixel) by default",
    },
    Param {
        label: "appearMargin",
        ty: "Length",
        doc: "grows the viewport on every side for `appeared` and `disappeared`, to start loading before the element is on screen; `0px` by default",
    },
];

/// Events every element fires, observed after mount and never on the
/// server (`docs/design/45-web-platform.md`, "Element events").
pub const UNIVERSAL_EVENTS: &[Event] = &[
    Event {
        name: "appeared",
        payload: "Void",
    },
    Event {
        name: "disappeared",
        payload: "Void",
    },
    Event {
        name: "resized",
        payload: "ElementSize",
    },
];

/// The `label:` attribute.
const LABEL: Param = Param {
    label: "label",
    ty: "String",
    doc: "a visible label on form controls; the accessible name of landmarks and groups",
};
/// The `role:` attribute (an `AriaRole`).
const ROLE: Param = Param {
    label: "role",
    ty: "AriaRole",
    doc: "a role no semantic element expresses (`.alert`, `.status`, …)",
};
/// The `disabled:` attribute of controls.
const DISABLED: Param = Param {
    label: "disabled",
    ty: "Bool",
    doc: "disables the control",
};
/// The `placeholder:` attribute of text inputs.
const PLACEHOLDER: Param = Param {
    label: "placeholder",
    ty: "String",
    doc: "the placeholder text of a text input",
};
/// The `autofocus:` attribute of form controls.
const AUTOFOCUS: Param = Param {
    label: "autofocus",
    ty: "Bool",
    doc: "moves the focus into the control when it appears: on the first render, and when a block inserts it later",
};
/// The `expanded:` attribute (`aria-expanded`) of buttons and links.
const EXPANDED: Param = Param {
    label: "expanded",
    ty: "Bool",
    doc: "`aria-expanded`",
};

/// The `click` event.
const CLICK: Event = Event {
    name: "click",
    payload: "Void",
};
/// The `focus` event.
const FOCUS: Event = Event {
    name: "focus",
    payload: "Void",
};
/// The `blur` event.
const BLUR: Event = Event {
    name: "blur",
    payload: "Void",
};
/// The pointer events of interactive elements: `click`, `mouseEnter`,
/// `mouseLeave`.
const POINTER: &[Event] = &[
    CLICK,
    Event {
        name: "mouseEnter",
        payload: "Void",
    },
    Event {
        name: "mouseLeave",
        payload: "Void",
    },
];
/// The focus events of form controls.
const CONTROL_EVENTS: &[Event] = &[FOCUS, BLUR];

/// The attributes of plain content elements.
const CONTENT_ATTRIBUTES: &[Param] = &[ROLE, LABEL];
/// The attributes of landmark elements (`main`, `nav`, …).
const LANDMARK_ATTRIBUTES: &[Param] = &[LABEL, ROLE];
/// The attributes of text inputs.
const TEXT_INPUT_ATTRIBUTES: &[Param] = &[LABEL, PLACEHOLDER, DISABLED, AUTOFOCUS];
/// The attributes of other form controls.
const CONTROL_ATTRIBUTES: &[Param] = &[LABEL, DISABLED, AUTOFOCUS];

/// Defines a plain content element with no required arguments.
const fn content(name: &'static str, html: &'static str) -> ElementDef {
    ElementDef {
        name,
        html,
        forms: &[],
        attributes: CONTENT_ATTRIBUTES,
        events: POINTER,
        content: Content::Any,
        category: Category::Content,
    }
}

/// Defines a landmark or grouping element.
const fn landmark(name: &'static str, html: &'static str) -> ElementDef {
    ElementDef {
        name,
        html,
        forms: &[],
        attributes: LANDMARK_ATTRIBUTES,
        events: &[],
        content: Content::Any,
        category: Category::Landmark,
    }
}

/// Every built-in element, sorted by name.
pub const ELEMENTS: &[ElementDef] = &[
    landmark("article", "article"),
    landmark("aside", "aside"),
    ElementDef {
        name: "audio",
        html: "audio",
        forms: &[&[Param {
            label: "source",
            ty: "Url",
            doc: "the audio file",
        }]],
        attributes: &[],
        events: &[],
        content: Content::None,
        category: Category::Media,
    },
    content("box", "div"),
    ElementDef {
        name: "button",
        html: "button",
        forms: &[],
        attributes: &[
            Param {
                label: "type",
                ty: "ButtonType",
                doc: "`.submit`, `.button` or `.reset`; `.button` on a non-submitting button in a form",
            },
            Param {
                label: "pressed",
                ty: "Bool",
                doc: "`aria-pressed`, for toggle buttons",
            },
            EXPANDED,
            DISABLED,
        ],
        events: &[CLICK, FOCUS, BLUR],
        content: Content::Any,
        category: Category::Interactive,
    },
    ElementDef {
        name: "canvas",
        html: "canvas",
        forms: &[],
        attributes: &[LABEL],
        events: POINTER,
        content: Content::None,
        category: Category::Media,
    },
    content("caption", "figcaption"),
    content("cell", "td"),
    ElementDef {
        name: "checkbox",
        html: "input",
        forms: &[&[Param {
            label: "isChecked",
            ty: "Binding<Bool>",
            doc: "the checked state, bound with `bind`",
        }]],
        attributes: CONTROL_ATTRIBUTES,
        events: CONTROL_EVENTS,
        content: Content::None,
        category: Category::FormControl,
    },
    content("code", "code"),
    content("codeBlock", "pre"),
    ElementDef {
        name: "dateField",
        html: "input",
        forms: &[&[Param {
            label: "value",
            ty: "Binding<Date?>",
            doc: "the chosen date",
        }]],
        attributes: CONTROL_ATTRIBUTES,
        events: CONTROL_EVENTS,
        content: Content::None,
        category: Category::FormControl,
    },
    content("definition", "dd"),
    ElementDef {
        name: "definitionList",
        html: "dl",
        forms: &[],
        attributes: LANDMARK_ATTRIBUTES,
        events: &[],
        content: Content::Only(&["term", "definition"]),
        category: Category::Landmark,
    },
    ElementDef {
        name: "divider",
        html: "hr",
        forms: &[],
        attributes: &[],
        events: &[],
        content: Content::None,
        category: Category::Content,
    },
    ElementDef {
        name: "dropdown",
        html: "select",
        forms: &[&[Param {
            label: "selection",
            ty: "Binding<T>",
            doc: "the selected option's value",
        }]],
        attributes: CONTROL_ATTRIBUTES,
        events: CONTROL_EVENTS,
        content: Content::Only(&["option"]),
        category: Category::FormControl,
    },
    content("emphasis", "em"),
    landmark("figure", "figure"),
    landmark("footer", "footer"),
    ElementDef {
        name: "form",
        html: "form",
        forms: &[],
        attributes: LANDMARK_ATTRIBUTES,
        events: &[Event {
            name: "submit",
            payload: "Void",
        }],
        content: Content::Any,
        category: Category::Landmark,
    },
    landmark("header", "header"),
    content("headerCell", "th"),
    ElementDef {
        name: "heading",
        html: "h1–h6",
        forms: &[&[Param {
            label: "level",
            ty: "Int",
            doc: "the heading level, an integer literal from 1 to 6",
        }]],
        attributes: CONTENT_ATTRIBUTES,
        events: POINTER,
        content: Content::Any,
        category: Category::Content,
    },
    ElementDef {
        name: "image",
        html: "img",
        forms: &[&[
            Param {
                label: "source",
                ty: "Url",
                doc: "the image",
            },
            Param {
                label: "description",
                ty: "String",
                doc: "the text alternative (`alt`); required",
            },
        ]],
        attributes: &[Param {
            label: "loading",
            ty: "ImageLoading",
            doc: "`.lazy` or `.eager`",
        }],
        events: POINTER,
        content: Content::None,
        category: Category::Media,
    },
    content("item", "li"),
    content("label", "label"),
    ElementDef {
        name: "lineBreak",
        html: "br",
        forms: &[],
        attributes: &[],
        events: &[],
        content: Content::None,
        category: Category::Content,
    },
    ElementDef {
        name: "link",
        html: "a",
        forms: &[
            &[Param {
                label: "to",
                ty: "Page",
                doc: "a page of the app",
            }],
            &[Param {
                label: "url",
                ty: "Url",
                doc: "an external URL",
            }],
        ],
        attributes: &[EXPANDED],
        events: &[CLICK, FOCUS, BLUR],
        content: Content::Any,
        category: Category::Interactive,
    },
    ElementDef {
        name: "list",
        html: "ul",
        forms: &[],
        attributes: LANDMARK_ATTRIBUTES,
        events: &[],
        content: Content::Only(&["item"]),
        category: Category::Landmark,
    },
    landmark("main", "main"),
    landmark("nav", "nav"),
    ElementDef {
        name: "numberField",
        html: "input",
        forms: &[&[Param {
            label: "value",
            ty: "Binding<Int>",
            doc: "the number, bound with `bind`",
        }]],
        attributes: TEXT_INPUT_ATTRIBUTES,
        events: CONTROL_EVENTS,
        content: Content::None,
        category: Category::FormControl,
    },
    ElementDef {
        name: "numberedList",
        html: "ol",
        forms: &[],
        attributes: LANDMARK_ATTRIBUTES,
        events: &[],
        content: Content::Only(&["item"]),
        category: Category::Landmark,
    },
    ElementDef {
        name: "option",
        html: "option",
        forms: &[&[Param {
            label: "value",
            ty: "T",
            doc: "the value the enclosing dropdown's selection takes",
        }]],
        attributes: &[DISABLED],
        events: &[],
        content: Content::Any,
        category: Category::Content,
    },
    content("paragraph", "p"),
    ElementDef {
        name: "passwordField",
        html: "input",
        forms: &[&[Param {
            label: "value",
            ty: "Binding<String>",
            doc: "the text, bound with `bind`",
        }]],
        attributes: TEXT_INPUT_ATTRIBUTES,
        events: CONTROL_EVENTS,
        content: Content::None,
        category: Category::FormControl,
    },
    content("row", "tr"),
    landmark("section", "section"),
    content("subscript", "sub"),
    content("superscript", "sup"),
    ElementDef {
        name: "table",
        html: "table",
        forms: &[],
        attributes: LANDMARK_ATTRIBUTES,
        events: &[],
        content: Content::Only(&["row"]),
        category: Category::Landmark,
    },
    content("term", "dt"),
    content("text", "span"),
    ElementDef {
        name: "textArea",
        html: "textarea",
        forms: &[&[Param {
            label: "value",
            ty: "Binding<String>",
            doc: "the text, bound with `bind`",
        }]],
        attributes: &[
            LABEL,
            PLACEHOLDER,
            DISABLED,
            AUTOFOCUS,
            Param {
                label: "rows",
                ty: "Int",
                doc: "the number of visible text lines",
            },
        ],
        events: CONTROL_EVENTS,
        content: Content::None,
        category: Category::FormControl,
    },
    ElementDef {
        name: "textField",
        html: "input",
        forms: &[&[Param {
            label: "value",
            ty: "Binding<String>",
            doc: "the text, bound with `bind`; a typed `@Form` field binds as its raw text",
        }]],
        attributes: TEXT_INPUT_ATTRIBUTES,
        events: CONTROL_EVENTS,
        content: Content::None,
        category: Category::FormControl,
    },
    ElementDef {
        name: "video",
        html: "video",
        forms: &[&[Param {
            label: "source",
            ty: "Url",
            doc: "the video file",
        }]],
        attributes: &[],
        events: &[],
        content: Content::None,
        category: Category::Media,
    },
];

/// HTML element names that K spells differently, with the K element a
/// diagnostic suggests ("`a` → did you mean `link`?").
pub const HTML_NAMES: &[(&str, &str)] = &[
    ("a", "link"),
    ("b", "emphasis"),
    ("br", "lineBreak"),
    ("dd", "definition"),
    ("div", "box"),
    ("dl", "definitionList"),
    ("dt", "term"),
    ("em", "emphasis"),
    ("figcaption", "caption"),
    ("h1", "heading"),
    ("h2", "heading"),
    ("h3", "heading"),
    ("h4", "heading"),
    ("h5", "heading"),
    ("h6", "heading"),
    ("hr", "divider"),
    ("i", "emphasis"),
    ("img", "image"),
    ("input", "textField"),
    ("li", "item"),
    ("ol", "numberedList"),
    ("p", "paragraph"),
    ("pre", "codeBlock"),
    ("select", "dropdown"),
    ("span", "text"),
    ("sub", "subscript"),
    ("sup", "superscript"),
    ("td", "cell"),
    ("textarea", "textArea"),
    ("th", "headerCell"),
    ("tr", "row"),
    ("ul", "list"),
];

/// HTML attribute spellings and the K argument labels they stand for, sorted
/// by HTML name: `image(src:)` is K2004 with the fix `source`.
pub const HTML_ATTRIBUTES: &[(&str, &str)] = &[
    ("alt", "description"),
    ("checked", "isChecked"),
    ("href", "url"),
    ("selected", "selection"),
    ("src", "source"),
];

/// Returns the K label an HTML attribute name stands for, if any.
#[must_use]
pub fn html_attribute(html: &str) -> Option<&'static str> {
    HTML_ATTRIBUTES
        .binary_search_by(|(h, _)| h.cmp(&html))
        .ok()
        .map(|i| HTML_ATTRIBUTES[i].1)
}

/// Returns the element named `name`.
#[must_use]
pub fn element(name: &str) -> Option<&'static ElementDef> {
    ELEMENTS
        .binary_search_by(|e| e.name.cmp(name))
        .ok()
        .map(|i| &ELEMENTS[i])
}

/// Returns the K element to suggest for an HTML element name.
#[must_use]
pub fn html_suggestion(html: &str) -> Option<&'static str> {
    HTML_NAMES
        .binary_search_by(|(h, _)| h.cmp(&html))
        .ok()
        .map(|i| HTML_NAMES[i].1)
}

impl ElementDef {
    /// Returns the attribute `label` of this element, including the
    /// universal ones.
    #[must_use]
    pub fn attribute(&self, label: &str) -> Option<&'static Param> {
        let attributes: &'static [Param] = self.attributes;
        UNIVERSAL_ATTRIBUTES
            .iter()
            .chain(attributes)
            .find(|p| p.label == label)
    }

    /// Returns the event `name` of this element, including the universal
    /// ones.
    #[must_use]
    pub fn event(&self, name: &str) -> Option<&'static Event> {
        self.all_events().find(|e| e.name == name)
    }

    /// The events `.on(…)` accepts on this element: its own, then the
    /// [`UNIVERSAL_EVENTS`].
    pub fn all_events(&self) -> impl Iterator<Item = &'static Event> {
        let events: &'static [Event] = self.events;
        events.iter().chain(UNIVERSAL_EVENTS)
    }
}
