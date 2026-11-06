// `.empty(field: value, …)` prefills any subset of the fields, typed as the fields.

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
    @Param let name: String
    @Form var signup: SignupRequest = .empty(name: name, age: 18)
    //                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: corpus.SignupRequest

    view {
        textField(value: bind signup.name)
            .label("Name")
    }
}
