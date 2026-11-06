// `.empty` can't be passed as an ordinary value.

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

func register(accounts: Accounts) async throws ValidationError {
    try await accounts.signup(request: .empty)
}
