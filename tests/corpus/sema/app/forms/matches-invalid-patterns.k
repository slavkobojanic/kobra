// An invalid pattern is reported at its string literal.
import k.validation.Matches
import k.validation.Validatable

struct Codes implements Validatable {
    @Matches("(ab") var open: String
    @Matches("*a") var star: String
    @Matches("a{{2,1}") var range: String
    @Matches("^ok$") var fine: String
}
