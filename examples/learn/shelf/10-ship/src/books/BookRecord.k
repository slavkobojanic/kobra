import accounts.AccountId
import k.crypto.Uuid
import k.db.Id
import k.db.Index

/// A book as the server stores it: one row of the `book_record` table.
public entity BookRecord {
    /// The primary key, a random id the server picks when the book is added.
    @Id let id: Uuid
    /// The account whose shelf the book is on: `nil` for the books added
    /// before Shelf had accounts.
    @Index let ownerId: AccountId?
    /// The book's title.
    var title: String
    /// Who wrote it.
    var author: String
    /// When the book was added, which orders the list.
    let addedAt: Date
    /// What the reader wrote about the book.
    var notes: String = ""
    /// From 1 to 5 stars, or 0 before the book is rated.
    var rating: Int = 0
}
