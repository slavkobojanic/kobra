//! What each annotation means, as data.
//!
//! An annotation is a stdlib type marked `@Annotation`; its arguments are
//! checked like a call of that type's initializer. This table adds what the
//! K declaration can't say: **where** the annotation may appear and which
//! **mutability** it requires (`docs/design/01-principles.md`, "Mutability
//! is always written out"). A mismatch is a compile error with a quick fix
//! that swaps `let` and `var`.
//!
//! Every entry names a stdlib declaration, and every `@Annotation`
//! declaration outside `k.internal` has an entry; the tests check both.

/// The `let`/`var` an annotated field must use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mutability {
    /// The field must be `let`.
    Let,
    /// The field must be `var`.
    Var,
    /// The annotation doesn't go on fields, or allows either.
    Any,
}

/// A place an annotation may appear.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// A field of a component, page or layout.
    ViewField,
    /// A field of a component only.
    ComponentField,
    /// A field of a page only.
    PageField,
    /// A field of a layout only (`@Slot let content: View`, where the page
    /// renders).
    LayoutField,
    /// A field of a service.
    ServiceField,
    /// A field of a test group (`@Inject` only).
    TestField,
    /// A field of an endpoint page (`@Inject` only).
    EndpointField,
    /// A field of a repository (`@Inject` only).
    RepositoryField,
    /// A field of a `config`.
    ConfigField,
    /// A field of an `entity`.
    EntityField,
    /// A field of a struct (`Codable` or `Validatable`).
    StructField,
    /// A function of a component, page, layout or service.
    ViewFunction,
    /// A function of an interface.
    InterfaceFunction,
    /// A handler function of an endpoint page.
    EndpointHandler,
    /// A `page` declaration.
    Page,
    /// The `app` declaration.
    App,
    /// A `component` declaration.
    Component,
    /// A `service` declaration.
    Service,
    /// A `struct` declaration.
    Struct,
    /// A declaration whose format a lockfile records (protocol, entity,
    /// migration, storage key, job).
    Locked,
}

/// One annotation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AnnotationDef {
    /// The fully qualified name of the declaring stdlib type.
    pub name: &'static str,
    /// Where it may appear.
    pub targets: &'static [Target],
    /// The mutability it requires on a field.
    pub mutability: Mutability,
    /// The design document that specifies it.
    pub design: &'static str,
}

/// Builds an [`AnnotationDef`]; keeps the [`ANNOTATIONS`] table to one line
/// per annotation.
const fn def(
    name: &'static str,
    targets: &'static [Target],
    mutability: Mutability,
    design: &'static str,
) -> AnnotationDef {
    AnnotationDef {
        name,
        targets,
        mutability,
        design,
    }
}

use Mutability::{Any, Let, Var};
use Target as T;

/// Every annotation user code may write, sorted by qualified name.
pub const ANNOTATIONS: &[AnnotationDef] = &[
    def("k.concurrency.Worker", &[T::Service], Any, "55-workers.md"),
    def(
        "k.core.Binding",
        &[T::ComponentField],
        Var,
        "04-components.md",
    ),
    def("k.core.Breaking", &[T::Locked], Any, "01-principles.md"),
    def("k.core.Builder", &[T::Struct], Any, "01-principles.md"),
    def(
        "k.core.Delete",
        &[T::EndpointHandler],
        Any,
        "27-endpoint-pages.md",
    ),
    def(
        "k.core.Derived",
        &[T::ViewField, T::ServiceField],
        Let,
        "03-reactivity.md",
    ),
    def("k.core.Effect", &[T::ViewFunction], Any, "03-reactivity.md"),
    def(
        "k.core.Emits",
        &[T::ComponentField],
        Let,
        "04-components.md",
    ),
    def(
        "k.core.Env",
        &[T::ConfigField],
        Let,
        "25-config-and-secrets.md",
    ),
    def(
        "k.core.Get",
        &[T::EndpointHandler],
        Any,
        "27-endpoint-pages.md",
    ),
    def(
        "k.core.Inject",
        &[
            T::ViewField,
            T::ServiceField,
            T::TestField,
            T::EndpointField,
            T::RepositoryField,
        ],
        Let,
        "10-services.md",
    ),
    def("k.core.Layout", &[T::Page, T::App], Any, "12-routing.md"),
    def("k.core.Load", &[T::ViewField], Let, "14-async-data.md"),
    def("k.core.NoLayout", &[T::Page], Any, "23-app-structure.md"),
    def("k.core.Offline", &[T::App], Any, "54-offline.md"),
    def("k.core.Param", &[T::PageField], Let, "12-routing.md"),
    def(
        "k.core.Patch",
        &[T::EndpointHandler],
        Any,
        "27-endpoint-pages.md",
    ),
    def(
        "k.core.Post",
        &[T::EndpointHandler],
        Any,
        "27-endpoint-pages.md",
    ),
    def("k.core.Prop", &[T::ComponentField], Let, "04-components.md"),
    def(
        "k.core.Put",
        &[T::EndpointHandler],
        Any,
        "27-endpoint-pages.md",
    ),
    def(
        "k.core.Rendering",
        &[T::App, T::Component],
        Any,
        "47-client-rendered-components.md",
    ),
    def("k.core.Scope", &[T::Service], Any, "10-services.md"),
    def(
        "k.core.SignInPage",
        &[T::App],
        Any,
        "26-auth-and-sessions.md",
    ),
    def(
        "k.core.Slot",
        &[T::ComponentField, T::LayoutField],
        Let,
        "04-components.md",
    ),
    def(
        "k.core.State",
        &[T::ViewField, T::ServiceField],
        Var,
        "03-reactivity.md",
    ),
    def("k.core.Stylable", &[T::ComponentField], Let, "06-styles.md"),
    def(
        "k.core.UserAction",
        &[T::ComponentField, T::InterfaceFunction],
        Any,
        "45-web-platform.md",
    ),
    def("k.db.Id", &[T::EntityField], Let, "19-persistence.md"),
    def("k.db.Index", &[T::EntityField], Any, "19-persistence.md"),
    def("k.db.Relation", &[T::EntityField], Any, "19-persistence.md"),
    def("k.db.Unique", &[T::EntityField], Any, "19-persistence.md"),
    def("k.db.Version", &[T::EntityField], Var, "19-persistence.md"),
    def(
        "k.encoding.JsonKey",
        &[T::StructField],
        Any,
        "32-http-client.md",
    ),
    def(
        "k.server.JobName",
        &[T::Service],
        Any,
        "30-background-jobs.md",
    ),
    def(
        "k.server.Retry",
        &[T::Service],
        Any,
        "30-background-jobs.md",
    ),
    def(
        "k.server.Schedule",
        &[T::Service],
        Any,
        "30-background-jobs.md",
    ),
    def(
        "k.validation.Check",
        &[T::StructField],
        Any,
        "15-forms-and-validation.md",
    ),
    def(
        "k.validation.Email",
        &[T::StructField, T::ConfigField],
        Any,
        "15-forms-and-validation.md",
    ),
    def(
        "k.validation.Form",
        &[T::ViewField],
        Var,
        "15-forms-and-validation.md",
    ),
    def(
        "k.validation.Format",
        &[T::StructField],
        Any,
        "15-forms-and-validation.md",
    ),
    def(
        "k.validation.Length",
        &[T::StructField, T::ConfigField],
        Any,
        "15-forms-and-validation.md",
    ),
    def(
        "k.validation.Matches",
        &[T::StructField, T::ConfigField],
        Any,
        "15-forms-and-validation.md",
    ),
    def(
        "k.validation.NotBlank",
        &[T::StructField, T::ConfigField],
        Any,
        "15-forms-and-validation.md",
    ),
    def(
        "k.validation.Range",
        &[T::StructField, T::ConfigField],
        Any,
        "15-forms-and-validation.md",
    ),
];

/// Returns the annotation declared by the stdlib type `qualified`.
#[must_use]
pub fn annotation(qualified: &str) -> Option<&'static AnnotationDef> {
    ANNOTATIONS
        .binary_search_by(|a| a.name.cmp(qualified))
        .ok()
        .map(|i| &ANNOTATIONS[i])
}
