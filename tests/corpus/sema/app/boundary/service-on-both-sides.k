// A service that injects something server-only and something client-only would be both.
import k.storage.LocalStorage

entity Note {
    @Id let id: Int
}

repository NoteRepository for Note {}

service Sync {
    @Inject let notes: NoteRepository
    @Inject let storage: LocalStorage
}
