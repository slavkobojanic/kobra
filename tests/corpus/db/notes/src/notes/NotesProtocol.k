import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// Notes over rpc.
public protocol NotesProtocol {
    /// Creates a note with tags.
    rpc create(id: Uuid, owner: String, title: String, body: String, tags: List<String>) throws RpcOnlyError -> NoteSummary

    /// An owner's notes.
    rpc list(owner: String) throws RpcOnlyError -> List<NoteSummary>

    /// Pins or unpins; `false` when there is no such note.
    rpc pin(id: Uuid, owner: String, isPinned: Bool) throws RpcOnlyError -> Bool

    /// Retitles a note read at `revision`.
    rpc edit(id: Uuid, owner: String, title: String, revision: Int) throws EditError -> NoteSummary

    /// Deletes a note; `false` when there is none.
    rpc remove(id: Uuid, owner: String) throws RpcOnlyError -> Bool

    /// The number of an owner's notes and of all tags.
    rpc counts(owner: String) throws RpcOnlyError -> List<Int>

    /// Titles of a page of two notes.
    rpc page(owner: String, offset: Int) throws RpcOnlyError -> List<String>
}
