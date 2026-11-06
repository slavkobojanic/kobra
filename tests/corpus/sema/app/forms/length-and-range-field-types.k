// `@Length` applies to text and lists, `@Range` to numbers.
import k.validation.Length
import k.validation.Range
import k.validation.Validatable

struct Order implements Validatable {
    @Length(max: 3) var quantity: Int
    @Range(1...10) var note: String
    @Length(max: 200) var comment: String
    @Range(1...10) var count: Int
}
