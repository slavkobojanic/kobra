/// Why `TodoService` rejected a todo title.
///
/// Titles are checked after leading and trailing whitespace is removed, so
/// `"   "` is `.empty` rather than a title of three spaces.
public enum TitleError implements Error {
    /// The title was empty, or contained only whitespace.
    case empty

    /// The title was longer than `limit` characters.
    case tooLong(limit: Int)

    /// A short sentence explaining the problem, written to be shown next to
    /// the field the title was typed into.
    var message: String {
        return match self {
            case .empty { "Give the todo a title." }
            case .tooLong(let limit) { "Keep the title to {limit} characters or fewer." }
        }
    }
}
