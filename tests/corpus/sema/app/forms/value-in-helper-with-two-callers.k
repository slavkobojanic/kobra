// A helper that is also called outside the action can't read `value`.

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

    func send() async throws ValidationError {
        try await accounts.signup(request: signup.value)
    }

    func submit() async throws ValidationError {
        try await send()
    }

    func retry() async {
        do {
            try await send()
        } catch _ {
        }
    }

    view {
        form {
            button { "Retry" }
                .on(click: retry)
        }
        .on(submit: signup.handle(submit))
    }
}
