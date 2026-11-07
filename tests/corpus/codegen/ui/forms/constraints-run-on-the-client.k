// Field constraints are compiled into the field specs, so the client checks the same rules as the
// server; their messages are in the client bundle (15-forms-and-validation.md).
import k.validation.Length
import k.validation.Range
import k.validation.Validatable

struct Profile implements Codable, Validatable {
    @Length(min: 8, message: "Use at least 8 characters") var password: String
    @Range(13...120, message: "Enter an age from 13 to 120") var age: Int
}

component ProfileForm {
    @Form var profile: Profile = .empty

    view {
        form {
            passwordField(value: bind profile.password)
                .label("Password")

            numberField(value: bind profile.age)
                .label("Age")
        }
    }
}
