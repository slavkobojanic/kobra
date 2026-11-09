import k.crypto.Uuid
import k.db.Id

/// A book as the server stores it: one row of the `book_record` table.
public entity BookRecord {
    /// The primary key, a random id the server picks when the book is added.
    @Id let id: Uuid
    /// The book's title.
    var title: String
    /// Who wrote it.
    var author: String
    /// When the book was added, which orders the list.
    let addedAt: Date
}
