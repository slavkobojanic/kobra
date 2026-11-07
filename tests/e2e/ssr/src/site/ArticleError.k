/// Why an article couldn't be loaded.
public enum ArticleError implements Error, Codable {
    /// No article has the slug.
    case notFound

    /// The article exists but is hidden.
    case hidden

    /// The article is being edited.
    case locked(until: String)
}
