// `.empty(name: name, age: 18)` prefills fields as raw text: the form holds text as typed, so the
// `Int` 18 starts as the text "18" (15-forms-and-validation.md, "The form's initial value").
import k.validation.Length
import k.validation.Validatable

struct SignupRequest implements Codable, Validatable {
    @Length(min: 3) var name: String
    var age: Int
}

component Signup {
    @Prop let suggested: String
    @Form var signup: SignupRequest = .empty(name: suggested, age: 18)

    view {
        textField(value: bind signup.name)
            .label("Name")
    }
}
