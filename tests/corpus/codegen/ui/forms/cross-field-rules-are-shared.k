// `validate(rules:)` is ordinary shared code: it runs in the browser's form and on the server for
// a `Validatable` parameter (15-forms-and-validation.md).
import k.validation.Rules
import k.validation.Validatable

struct Passwords implements Codable, Validatable {
    var password: String
    var confirmPassword: String

    func validate(rules: Rules) {
        rules.check(confirmPassword == password, field: .confirmPassword, message: "Passwords must match")
    }
}

component Reset {
    @Form var passwords: Passwords = .empty

    view {
        form {
            passwordField(value: bind passwords.password)
                .label("Password")

            passwordField(value: bind passwords.confirmPassword)
                .label("Confirm password")
        }
    }
}
