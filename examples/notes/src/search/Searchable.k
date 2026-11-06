/// Something a person can find by typing part of its text.
///
/// A conforming type supplies the text to search, and the default
/// `matches(query:)` does the matching, so every searchable list in the app
/// behaves the same way: letter case and surrounding spaces are ignored, and
/// a blank query matches everything.
public interface Searchable {
    /// The pieces of text a query is matched against, such as a title and a
    /// body.
    func searchableText() -> List<String>

    /// Whether the query appears in any piece of `searchableText()`.
    ///
    /// - Parameter query: the text as typed.
    /// - Returns: `true` when the query is blank, or when any piece contains
    ///   it, ignoring letter case.
    func matches(query: String) -> Bool {
        let term = query.trimmed().lowercased()

        guard term.isNotEmpty else {
            return true
        }

        return searchableText().any { text => text.lowercased().contains(term) }
    }
}
