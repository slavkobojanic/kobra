// With `@Breaking` on the key, the incompatible change is allowed.
import k.storage.LocalStorage
import k.storage.StorageKey

/// Keeps the cart across visits.
public service CartService {
    @Inject let storage: LocalStorage

    @Breaking("the cart is rebuilt from the server")
    private let savedCart: StorageKey<List<SavedItem>> = StorageKey(name: "cart")
}
