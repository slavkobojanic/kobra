// A key entry without a type: K8002.
import k.storage.LocalStorage
import k.storage.StorageKey

/// Keeps the cart across visits.
public service CartService {
    @Inject let storage: LocalStorage

    private let savedCart: StorageKey<List<SavedItem>> = StorageKey(name: "cart")
}
