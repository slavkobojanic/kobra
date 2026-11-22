//! The [`Diagnostic`] type and its parts.

use std::fmt;

use crate::codes::Code;
use crate::span::Span;

/// How serious a diagnostic is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Severity {
    /// The program is invalid. `k check` and `k build` fail.
    Error,
    /// The program is valid but probably not what was meant, for example an
    /// effect that reads no reactive values. `k check` fails on warnings in
    /// CI mode as well; the distinction matters for `k build` and the editor.
    Warning,
}

impl Severity {
    /// Returns the lowercase name used in rendered output: `error` or
    /// `warning`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A secondary location attached to a diagnostic, with an optional message.
///
/// Labels point at related code: "the string starts here", "first defined
/// here", "this `@Prop` is read-only".
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Label {
    /// The location the label points at.
    pub span: Span,
    /// The text shown next to the location. May be empty.
    pub message: String,
}

/// How safely a [`Fix`] can be applied without human review.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Applicability {
    /// The fix is certainly what the user meant and produces valid code. An
    /// agent or editor may apply it automatically.
    MachineApplicable,
    /// The fix is a likely improvement but may change meaning or be
    /// incomplete. It must be reviewed before it is applied.
    MaybeIncorrect,
}

impl Applicability {
    /// Returns the kebab-case name used in JSON output and `.diag` files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Applicability::MachineApplicable => "machine-applicable",
            Applicability::MaybeIncorrect => "maybe-incorrect",
        }
    }
}

/// A single text replacement: replace the text covered by `span` with
/// `replacement`. An empty span inserts; an empty replacement deletes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TextEdit {
    /// The text to replace.
    pub span: Span,
    /// The new text.
    pub replacement: String,
}

/// A quick fix: a described set of edits that resolves a diagnostic.
///
/// The edits of one fix never overlap and are applied together. They are
/// stored in source order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fix {
    /// A short imperative description, for example "Replace `&&` with `and`".
    pub message: String,
    /// The edits that make up the fix.
    pub edits: Vec<TextEdit>,
    /// Whether the fix may be applied without review.
    pub applicability: Applicability,
}

impl Fix {
    /// Creates a machine-applicable fix consisting of a single edit.
    #[must_use]
    pub fn replace(message: impl Into<String>, span: Span, replacement: impl Into<String>) -> Fix {
        Fix {
            message: message.into(),
            edits: vec![TextEdit {
                span,
                replacement: replacement.into(),
            }],
            applicability: Applicability::MachineApplicable,
        }
    }

    /// Returns this fix with its applicability set to
    /// [`Applicability::MaybeIncorrect`].
    #[must_use]
    pub fn maybe_incorrect(mut self) -> Fix {
        self.applicability = Applicability::MaybeIncorrect;
        self
    }
}

/// A structured problem report about user input.
///
/// See the [crate documentation](crate) for an overview. Diagnostics are
/// built with [`Diagnostic::new`] and the `with_*` methods:
///
/// ```
/// use k_diagnostics::{codes, Diagnostic, FileId, Fix, Span, TextRange};
///
/// let file = FileId::new(0);
/// let at = Span::new(file, TextRange::new(10.into(), 12.into()));
/// let diagnostic = Diagnostic::new(codes::SYMBOLIC_AND, at, "Kobra uses `and`, not `&&`")
///     .with_fix(Fix::replace("Replace `&&` with `and`", at, "and"));
/// assert_eq!(diagnostic.code.to_string(), "K0104");
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    /// The stable code identifying the kind of problem.
    pub code: Code,
    /// How serious the problem is.
    pub severity: Severity,
    /// A one-line description. It starts with a lowercase letter and has no
    /// trailing period, like `rustc` messages.
    pub message: String,
    /// The primary location of the problem.
    pub span: Span,
    /// Secondary locations, in the order they were added.
    pub labels: Vec<Label>,
    /// Additional context ("note: ...").
    pub notes: Vec<String>,
    /// Suggestions in prose ("help: ...").
    pub helps: Vec<String>,
    /// Quick fixes, most likely first.
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    /// Creates a diagnostic with the default severity registered for `code`.
    ///
    /// Codes missing from the registry default to [`Severity::Error`]; the
    /// registry test in this crate ensures every code constant is registered.
    #[must_use]
    pub fn new(code: Code, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code,
            severity: code.default_severity(),
            message: message.into(),
            span,
            labels: Vec::new(),
            notes: Vec::new(),
            helps: Vec::new(),
            fixes: Vec::new(),
        }
    }

    /// Returns this diagnostic with its severity replaced.
    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Diagnostic {
        self.severity = severity;
        self
    }

    /// Returns this diagnostic with a secondary label added.
    #[must_use]
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Returns this diagnostic with a note added.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Diagnostic {
        self.notes.push(note.into());
        self
    }

    /// Returns this diagnostic with a help line added.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Diagnostic {
        self.helps.push(help.into());
        self
    }

    /// Returns this diagnostic with a quick fix added.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Diagnostic {
        self.fixes.push(fix);
        self
    }

    /// Returns `true` if this diagnostic is an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}
