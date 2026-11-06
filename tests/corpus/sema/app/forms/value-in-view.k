// `value` can't be read in the view, where the fields may not parse.

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

    view {
        text { signup.value.name }
    }
}
