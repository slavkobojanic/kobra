import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.time.Clock

/// The server's side of `BookProtocol`.
public service BookApi implements BookProtocol {
    @Inject let records: BookRepository
    @Inject let clock: Clock

    rpc list() async throws RpcOnlyError -> List<Book> {
        let all = await records.all()

        return all.map { record => Book(id: record.id, title: record.title, author: record.author) }
    }

    rpc add(draft: BookDraft) async throws RpcOnlyError -> Book {
        let record = BookRecord(
            id: Uuid.random(),
            title: draft.title,
            author: draft.author,
            addedAt: clock.now
        )

        await records.insert(record)

        return Book(id: record.id, title: record.title, author: record.author)
    }
}
