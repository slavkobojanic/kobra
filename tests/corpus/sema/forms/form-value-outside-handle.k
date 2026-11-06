import k.validation.NotBlank
import k.validation.Validatable

struct SignInRequest implements Codable, Validatable {
    @NotBlank var email: String
    @NotBlank var password: String
}

page SignIn {
    @Form var credentials: SignInRequest = .empty

    func submit() async {
        let request = credentials.value
    }

    func peek() -> String {
        return credentials.value.email
    }

    view {
        form {
            button { "Sign in" }.type(.submit)
        }
        .on(submit: credentials.handle(submit))
    }
}
