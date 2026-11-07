import k.rpc.RpcOnlyError

public service NotesApi implements NotesProtocol {
    @Inject let notes: NoteRepository
    @Inject let config: SearchConfig

    rpc find(title: String) async throws RpcOnlyError -> List<NoteSummary> {
        let found = await notes.titled(title: "server-only literal {title}")
        return found.map { note => NoteSummary(title: note.title) }
    }
}
