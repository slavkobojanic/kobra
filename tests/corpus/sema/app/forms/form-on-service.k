// `@Form` belongs to pages and components, not services.

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

service Drafts {
    @Form var signup: SignupRequest = .empty
}
