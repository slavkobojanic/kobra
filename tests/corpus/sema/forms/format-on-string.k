import k.text.EmailAddress
import k.validation.Format
import k.validation.Validatable

struct Contact implements Validatable {
    @Format(message: "Enter a valid email address") var email: EmailAddress
    @Format(message: "Enter a name") var name: String
}
