/// The `book_record` table, for the server's code.
public repository BookRepository for BookRecord {
    /// Returns every book on the shelf, the first one added first.
    func all() async -> List<BookRecord> {
        return await bookRecords.sortedBy { record => record.addedAt }
    }
}
