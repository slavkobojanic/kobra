// `@Matches` patterns are ECMAScript regular expressions, checked at compile time. A literal `{` is
// written `{{`, so a quantifier is `{{3}`.
import k.validation.Matches
import k.validation.Validatable

struct Codes implements Validatable {
    @Matches("^[a-z0-9-]+$") var slug: String
    @Matches("^\\d{{3}-\\d{{4}$") var phone: String
    @Matches("^(?:[A-Z]{{2}){{1,3}$", message: "Two to six capitals") var region: String
}
