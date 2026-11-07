/// Why a board's or a column's title was rejected.
public enum TitleError implements Error, Equatable {
    /// The title was empty, or only whitespace.
    case empty

    /// The title was longer than `limit` characters.
    case tooLong(limit: Int)

    /// A sentence to show next to the field.
    var message: String {
        return match self {
            case .empty { "Give it a name." }
            case .tooLong(let limit) { "Keep the name to {limit} characters or fewer." }
        }
    }
}
