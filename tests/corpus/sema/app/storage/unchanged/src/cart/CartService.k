// Both keys match storage.lock: no diagnostics.
import k.storage.LocalStorage
import k.storage.StorageKey

/// Keeps the cart across visits.
public service CartService {
    @Inject let storage: LocalStorage

    private let savedCart: StorageKey<List<SavedItem>> = StorageKey(name: "cart")
}
