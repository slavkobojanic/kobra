// `@Form` fields are bindable: inputs bind form fields directly
// (15-forms-and-validation.md).
import k.validation.Form
import k.validation.Validatable

struct Signup implements Validatable {
    var email: String
}

component SignupForm {
    @Form var signup: Signup = .empty

    view {
        textField(value: bind signup.email)
            .label("Email")
    }
}
