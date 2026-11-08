// `@Check(Service.function)` runs in the browser too, debounced on blur
// (15-forms-and-validation.md): the field spec carries the rule as a call of
// the service in the component's container, with the form's cancellation
// signal and the parsed value, and the form debounces it on the app's `Clock`.
import k.text.EmailAddress
import k.validation.Check
import k.validation.Validatable

service Accounts {
    func emailAvailable(email: EmailAddress) async -> Bool {
        return true
    }
}

struct Signup implements Codable, Validatable {
    @Check(Accounts.emailAvailable, message: "That address is taken")
    var email: EmailAddress
    var name: String
}

component SignupForm {
    @Form var signup: Signup = .empty

    view {
        form {
            textField(value: bind signup.email)
                .label("Email")
        }
    }
}
