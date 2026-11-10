// A struct field with a default still decodes the stored values, as an
// optional one does: compatible, as in `rpc.lock`.
import k.storage.LocalStorage
import k.storage.StorageKey

/// Keeps the cart across visits.
public service CartService {
    @Inject let storage: LocalStorage

    private let savedCart: StorageKey<List<SavedItem>> = StorageKey(name: "cart")
}
