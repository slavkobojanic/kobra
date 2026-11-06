// A lambda passed to `handle` may read `value`.

import k.validation.Length
import k.validation.Validatable
import k.validation.ValidationError

struct SignupRequest implements Codable, Validatable {
    @Length(min: 3) var name: String
    var age: Int
    var newsletter: Bool = false
}

service Accounts {
    func signup(request: SignupRequest) async throws ValidationError {
    }
}

component SignupForm {
    @Inject let accounts: Accounts
    @Form var signup: SignupRequest = .empty

    view {
        form {
            button { "Sign up" }
                .type(.submit)
        }
        .on(submit: signup.handle { try await accounts.signup(request: signup.value) })
    }
}
