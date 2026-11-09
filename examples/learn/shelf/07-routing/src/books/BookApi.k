import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.time.Clock

/// The server's side of `BookProtocol`.
public service BookApi implements BookProtocol {
    @Inject let records: BookRepository
    @Inject let clock: Clock

    rpc list() async throws RpcOnlyError -> List<Book> {
        let all = await records.all()

        return all.map { record => book(record: record) }
    }

    rpc get(id: Uuid) async throws LookupError -> Book {
        guard let record = await records.find(id: id) else {
            throw LookupError.notFound
        }

        return book(record: record)
    }

    rpc add(draft: BookDraft) async throws RpcOnlyError -> Book {
        let record = BookRecord(
            id: Uuid.random(),
            title: draft.title,
            author: draft.author,
            addedAt: clock.now
        )

        await records.insert(record)

        return book(record: record)
    }

    rpc review(id: Uuid, review: BookReview) async throws LookupError -> Book {
        guard let record = await records.find(id: id) else {
            throw LookupError.notFound
        }

        var reviewed = record
        reviewed.notes = review.notes
        reviewed.rating = review.rating

        await records.update(reviewed)

        return book(record: reviewed)
    }

    /// The book the browser shows for a stored record.
    private func book(record: BookRecord) -> Book {
        return Book(
            id: record.id,
            title: record.title,
            author: record.author,
            notes: record.notes,
            rating: record.rating
        )
    }
}
