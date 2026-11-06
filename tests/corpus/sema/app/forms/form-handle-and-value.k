// `@Form var f: S = .empty`, `f.handle(action)` and `f.value` read inside the action.

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

page Signup {
    @Inject let accounts: Accounts
    @Form var signup: SignupRequest = .empty
    //                                ^^^^^^ type: corpus.SignupRequest

    func submit() async throws ValidationError {
        try await accounts.signup(request: signup.value)
        //                                 ^^^^^^^^^^^^ type: corpus.SignupRequest
    }

    view {
        form {
            textField(value: bind signup.name)
                .label("Name")
            button { "Sign up" }
                .type(.submit)
                .disabled(signup.isSubmitting)
        }
        .on(submit: signup.handle(submit))
        //          ^^^^^^^^^^^^^^^^^^^^^ type: () async -> Void
    }
}
