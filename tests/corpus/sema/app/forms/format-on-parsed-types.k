// `@Format(message:)` applies to fields of types parsed from text.
import k.crypto.Uuid
import k.text.EmailAddress
import k.validation.Format
import k.validation.Validatable

struct Invite implements Validatable {
    @Format(message: "Enter a valid email address") var email: EmailAddress
    @Format(message: "Enter a number") var seats: Int
    @Format(message: "Enter a web address") var site: Url
    @Format(message: "Enter an id") var team: Uuid
}
