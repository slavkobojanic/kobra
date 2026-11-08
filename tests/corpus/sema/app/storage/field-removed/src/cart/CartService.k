// Removing a field of the stored value type: K8008 at the key.
import k.storage.LocalStorage
import k.storage.StorageKey

/// Keeps the cart across visits.
public service CartService {
    @Inject let storage: LocalStorage

    private let savedCart: StorageKey<List<SavedItem>> = StorageKey(name: "cart")
}
