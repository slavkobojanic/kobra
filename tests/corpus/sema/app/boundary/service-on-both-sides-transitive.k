// The sides are inherited through injected services: only the service that joins them is reported.
import k.storage.LocalStorage

entity Note {
    @Id let id: Int
}

repository NoteRepository for Note {}

service ServerNotes {
    @Inject let notes: NoteRepository
}

service LocalDrafts {
    @Inject let storage: LocalStorage
}

service Sync {
    @Inject let server: ServerNotes
    @Inject let local: LocalDrafts
}
