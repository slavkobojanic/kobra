// Constraint annotations check their field types: `@Matches` needs text.
import k.validation.Matches
import k.validation.Validatable

struct Order implements Validatable {
    @Matches("^[0-9]+$") var quantity: Int
}
