import accounts.AccountId
import k.crypto.Uuid

/// The `book_record` table, for the server's code. Every query takes the
/// owner, so no one reads another person's books.
public repository BookRepository for BookRecord {
    /// Returns every book on an account's shelf, the first one added first.
    func all(ownerId: AccountId) async -> List<BookRecord> {
        return await bookRecords
            .filter { record => record.ownerId == ownerId }
            .sortedBy { record => record.addedAt }
    }

    /// Returns the account's book with this id, or `nil` when it has none.
    func find(id: Uuid, ownerId: AccountId) async -> BookRecord? {
        return await bookRecords.first { record => record.id == id and record.ownerId == ownerId }
    }
}
