import k.validation.Matches
import k.validation.Validatable

let pattern = "[a-z]+"

struct Code implements Validatable {
    @Matches("^[A-Z]{3}$") var fine: String
    @Matches("[a-z") var broken: String
    @Matches(pattern) var dynamic: String
}
