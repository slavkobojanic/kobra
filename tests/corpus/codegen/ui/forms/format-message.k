// `@Format(message:)` is the parse-failure message of a typed field (15-forms-and-validation.md,
// "Typed fields and @Format").
import k.text.EmailAddress
import k.validation.Format
import k.validation.Validatable

struct Invite implements Codable, Validatable {
    @Format(message: "Enter a valid email address") var email: EmailAddress
}

component InviteForm {
    @Form var invite: Invite = .empty

    view {
        textField(value: bind invite.email)
            .label("Email")
    }
}
