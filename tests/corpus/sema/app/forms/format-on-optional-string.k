// A `String?` can't fail to parse either.
import k.validation.Format
import k.validation.Validatable

struct Profile implements Validatable {
    @Format(message: "Enter a nickname") var nickname: String?
}
