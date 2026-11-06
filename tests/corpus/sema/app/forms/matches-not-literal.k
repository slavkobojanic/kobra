// `@Matches` takes a string literal: no interpolation, no concatenation, no constants.
import k.validation.Matches
import k.validation.Validatable

let prefix = "ab"

struct Codes implements Validatable {
    @Matches("^{prefix}$") var interpolated: String
    @Matches("^a" + "b$") var joined: String
    @Matches(prefix) var named: String
}
