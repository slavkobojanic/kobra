# Annotations

**Keywords declare kinds; annotations modify behavior within a kind.** Annotations start with `@`, come
before any modifiers (`@Inject private let api: CartProtocol`), and may take arguments. Documentation
comments come before annotations.

## Reactivity

| Annotation | Field | Meaning |
|---|---|---|
| `@State` | `var` | mutable reactive value |
| `@Derived` | `let` | read-only value computed from reactive values |
| `@Effect`, `@Effect(on: […])` | function | side effect that re-runs when what it read changes, or when the listed values change; never `throws` |
| `@Load` | `let` | async derived value; the view waits for it, and `reload()` re-runs it |

## Component API

| Annotation | Field | Meaning |
|---|---|---|
| `@Prop` | `let` | one-way input from the parent |
| `@Binding` | `var` | two-way input; the parent passes `bind x` |
| `@Emits` | `let` | an event, of type `Event<T>`; the parent listens with `.on(name: handler)` |
| `@Slot` | `let` | content from the parent, of type `View` or `(T) -> View` |
| `@Stylable` | `let` | a class hook the parent may style, of type `Classes` |

## Services and injection

| Annotation | On | Meaning |
|---|---|---|
| `@Inject` | `let` field | injected dependency, resolved at compile time; always private |
| `@Scope(.app / .request / .route / .component / .server)` | service | lifetime of instances; `.app` is the default, `.server` is one per process |

## Pages and routing

| Annotation | On | Meaning |
|---|---|---|
| `@Param` | `let` field | a page parameter; required without a default, optional with one |
| `@Layout(Name)` | page, app | the layout that wraps the page, or every view page |
| `@NoLayout` | page | opts out of the app's default layout |
| `@SignInPage(.page)` | app | the `Page` the `Authenticated` guard redirects to |
| `@Rendering(.client / .server)` | app | server rendering on or off |
| `@Get`, `@Post`, `@Put`, `@Patch`, `@Delete` | function | HTTP handlers of an endpoint page |

## Forms and validation

| Annotation | On | Meaning |
|---|---|---|
| `@Form` | `var` field | wraps a `Validatable` struct with per-field form state; starts from the generated `.empty` or `.empty(field: value, …)` |
| `@Email`, `@NotBlank` | field | the value is an email address, or not blank; `@Email` on an `EmailAddress` field is a warning |
| `@Length(min:max:)` | field | string length |
| `@Range(a...b)` | field | numeric range |
| `@Matches("^[a-z0-9-]+$")` | field | a regular expression, as a string literal checked at compile time |
| `@Format(message:)` | field of a parsed type | the message shown when the text doesn't parse; a compile error on a `String` field |
| `@Check(Service.function)` | field | an async rule run by a service |

All constraints accept `message:`, and they apply to `config` fields too, `Secret<T>` fields included.

## Persistence

| Annotation | On | Meaning |
|---|---|---|
| `@Id` | entity field | primary key |
| `@Unique` | entity field | unique index; `insert` and `update` throw a typed `UniqueViolation` |
| `@Index` | entity field | single-column index |
| `@Relation`, `@Relation(inverse: …)` | entity field | relation to another entity |
| `@Version` | `var` entity field | optimistic locking; `update` returns `false` on a conflict |

## Types and compatibility

| Annotation | On | Meaning |
|---|---|---|
| `@Builder` | struct | required fields are named arguments, the rest are chained |
| `@Breaking("reason")` | protocol, migration, job, storage key | acknowledges a deliberate breaking change to a locked value |
| `@JsonKey("name")` | `Codable` field | the JSON key, when it differs from the field name |

## Configuration and jobs

| Annotation | On | Meaning |
|---|---|---|
| `@Env("NAME")` | `let` config field | the environment variable the field is read from |
| `@JobName("name")` | job service | the stable name of the job's queue rows |
| `@Retry(attempts:backoff:)` | job service | retry policy |
| `@Schedule(…)` | job service | recurring schedule: `.every(…)`, `.daily(at:)`, `.weekly(on:at:)` |

## Mutability

The annotation decides the keyword, and a mismatch is a compile error with a quick fix:

| Must be `var` | Must be `let` |
|---|---|
| `@State`, `@Binding`, `@Form`, `@Version` | `@Prop`, `@Derived`, `@Load`, `@Inject`, `@Param`, `@Slot`, `@Stylable`, `@Emits`, `@Env` |
