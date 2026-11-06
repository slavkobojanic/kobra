// `value` can't be read in a `@Derived` initializer either.

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
    @Form var signup: SignupRequest = .empty
    @Derived let greeting: String = "Hello, {signup.value.name}"

    view {
        text { greeting }
    }
}
