import k.validation.NotBlank
import k.validation.Validatable

struct SignInRequest implements Codable, Validatable {
    @NotBlank var email: String
    @NotBlank var password: String
}

let blank: SignInRequest = .empty
