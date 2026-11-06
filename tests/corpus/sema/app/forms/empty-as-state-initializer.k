// `.empty` is valid only as a `@Form` initializer, not as a `@State` one.

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
    @State var draft: SignupRequest = .empty

    view {
        text { draft.name }
    }
}
