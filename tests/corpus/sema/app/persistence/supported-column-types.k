// Column types: numbers, text, booleans, dates, ids, typed text, money, payload-free enums,
// optionals and relations.
import k.crypto.Uuid
import k.text.EmailAddress

enum Status {
    case open
    case closed
}

entity Tag {
    @Id let id: Uuid
    @Relation let order: Order
}

entity Order {
    @Id let id: Uuid
    @Unique var email: EmailAddress
    @Index var placedAt: Date
    var total: Money
    var status: Status = .open
    var note: String? = nil
    var count: Int
    var paid: Bool
    @Version var revision: Int = 1
    @Relation var tags: List<Tag> = []
}
