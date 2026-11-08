import k.storage.LocalStorage
import k.storage.StorageKey

/// Keeps a draft across visits.
public service DraftService {
    @Inject let storage: LocalStorage

    private let draft: StorageKey<String> = StorageKey(name: "todos")
}
