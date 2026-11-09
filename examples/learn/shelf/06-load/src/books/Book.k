import k.crypto.Uuid

/// A book on the reading list, as the server sends it to the browser.
public struct Book implements Codable {
    /// Tells the books apart, even two with the same title.
    let id: Uuid
    /// The book's title, as printed on the cover.
    let title: String
    /// Who wrote it.
    let author: String
}
