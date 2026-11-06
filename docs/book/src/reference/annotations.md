# Annotations

**Keywords declare kinds; annotations modify behavior within a kind.** Annotations start with `@`, come
before any modifiers (`@Inject private let api: CartProtocol`), and may take arguments.

Entries marked *proposal* follow a design proposal; see [Design status](design-status.md).

## Reactivity

| Annotation | Field | Meaning |
|---|---|---|
| `@State` | `var` | mutable reactive value |
| `@Derived` | `let` | read-only value computed from reactive values |
| `@Effect`, `@Effect(on: […])` | function | side effect that re-runs when what it read changes, or when the listed values change |
| `@Load` | `let` | async derived value; the view waits for it |

## Component API

| Annotation | Field | Meaning |
|---|---|---|
| `@Prop` | `let` | one-way input from the parent |
| `@Binding` | `var` | two-way input; the parent passes `bind x` |
| `@Emits` | `let` | an event, of type `Event<T>` |
| `@Slot` | `let` | content from the parent, of type `View` or `(T) -> View` |
| `@Stylable` | `let` | a class hook the parent may style, of type `Classes` |

## Services and injection

| Annotation | On | Meaning |
|---|---|---|
| `@Inject` | `let` field | injected dependency, resolved at compile time |
| `@Scope(.app / .request / .route / .component)` | service | lifetime of instances; `.app` is the default |
| `@Scope(.server)` | service | one instance per server process (*proposal*) |

## Pages and routing

| Annotation | On | Meaning |
|---|---|---|
| `@Param` | `let` field | a page parameter; required without a default, optional with one |
| `@Layout(Name)` | page, app | the layout that wraps the page, or every page (on `app`, *proposal*) |
| `@SignInPage(Name)` | app | where `Authenticated` sends visitors (*proposal*) |
| `@Rendering(.client / .server)` | app | server rendering on or off (*proposal*) |
| `@Get`, `@Post`, `@Put`, `@Patch`, `@Delete` | function | HTTP handlers of an endpoint page (*proposal*) |

## Forms and validation

| Annotation | On | Meaning |
|---|---|---|
| `@Form` | `var` field | wraps a `Validatable` struct with per-field form state |
| `@Email`, `@NotBlank` | field | the value is an email address, or not blank |
| `@Length(min:max:)` | field | string length |
| `@Range(a...b)` | field | numeric range |
| `@Matches(pattern)` | field | regular expression |
| `@Check(Service.function)` | field | an async rule run by a service |

All constraints accept `message:`.

## Persistence

| Annotation | On | Meaning |
|---|---|---|
| `@Id` | entity field | primary key |
| `@Unique` | entity field | unique constraint |
| `@Relation` | entity field | relation to another entity |
| `@Index` | entity field | single-column index (*proposal*) |

## Types and compatibility

| Annotation | On | Meaning |
|---|---|---|
| `@Builder` | struct | required fields are named arguments, the rest are chained |
| `@Breaking("reason")` | protocol, migration | acknowledges a deliberate breaking change |
| `@JsonKey("name")` | `Codable` field | the JSON key, when it differs from the field name (*proposal*) |

## Configuration and jobs

| Annotation | On | Meaning |
|---|---|---|
| `@Env("NAME")` | config field | the environment variable (*proposal*) |
| `@Retry(attempts:backoff:)` | job service | retry policy (*proposal*) |
| `@Schedule(…)` | job service | recurring schedule (*proposal*) |

## Mutability

The annotation decides the keyword, and a mismatch is a compile error with a quick fix:

| Must be `var` | Must be `let` |
|---|---|
| `@State`, `@Binding`, `@Form` | `@Prop`, `@Derived`, `@Load`, `@Inject`, `@Param`, `@Slot`, `@Stylable`, `@Emits`, `@Env` |
