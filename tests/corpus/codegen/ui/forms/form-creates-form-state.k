// `@Form var signup: SignupRequest = .empty` creates the form state with one field spec per stored
// field, in declaration order, every field starting blank (15-forms-and-validation.md,
// runtime.md §9).
import k.validation.Length
import k.validation.Validatable
import k.validation.ValidationError

struct SignupRequest implements Codable, Validatable {
    @Length(min: 3) var name: String
    var age: Int
    var newsletter: Bool = false
}

component Signup {
    @Form var signup: SignupRequest = .empty

    func submit() async throws ValidationError {
    }

    view {
        form {
            textField(value: bind signup.name)
                .label("Name")

            numberField(value: bind signup.age)
                .label("Age")

            button { "Sign up" }
                .type(.submit)
        }
        .on(submit: signup.handle(submit))
    }
}
