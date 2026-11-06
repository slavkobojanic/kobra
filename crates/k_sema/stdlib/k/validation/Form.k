/// A form over a `Validatable` struct, declared with
/// `@Form var signup: SignupRequest = .empty`
/// (`docs/design/15-forms-and-validation.md`). Requires `var`.
///
/// The field's type is written as the struct `T`; its value has type
/// `Form<T>`. `signup.email` is the raw input of a field, bindable with
/// `bind`, and `T.empty` / `T.empty(field: value, …)` is generated and valid
/// only as a `@Form` initializer.
@Annotation
@Native
public class Form<T> where T implements Validatable {
    /// The parsed struct; readable only inside the action passed to
    /// `handle`.
    var value: T
    /// `true` while the action passed to `handle` runs.
    var isSubmitting: Bool
    /// `true` if every field parses and passes its constraints.
    var isValid: Bool
    /// `true` once any field has been edited.
    var isTouched: Bool
    /// The current failures, by field path.
    var errors: List<FieldError>

    /// A submit handler that validates first, sets `isSubmitting`, prevents
    /// double submits, and shows a `ValidationError` the action throws under
    /// its fields. The returned handler doesn't throw.
    func handle(_ action: () async throws ValidationError -> Void) -> () async -> Void
}
