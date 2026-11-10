import k.device.TabChannel
import k.device.TabMessaging
import k.storage.LocalStorage
import k.storage.StorageKey

/// The cart's changes, from one tab to the shop's other tabs. A channel pairs
/// its name with the message type, so every tab posts and reads `CartChange`.
let cartChanges: TabChannel<CartChange> = TabChannel(name: "cart")

/// The shopper's cart, kept in the browser's local storage and in step across
/// the shop's open tabs: adding a mug in one tab shows it in the others' headers
/// at once.
///
/// Injecting `LocalStorage` and `TabMessaging` makes this service client-only,
/// so the components that show it are rendered in the browser (`CartBadge`,
/// `AddToCart`).
public service CartService {
    @Inject let storage: LocalStorage
    @Inject let tabs: TabMessaging

    // The key's name and the shape of `CartItem` are recorded in storage.lock.
    private let savedCart = StorageKey<List<CartItem>>(name: "cart")

    /// The products in the cart, oldest first; a product added twice is in it twice.
    @State var items: List<CartItem> = []

    /// Restores the saved cart and starts listening to the other tabs. The
    /// service lives as long as the tab, so the subscription may too.
    init() {
        do {
            items = try storage.read(key: savedCart) ?? []
        } catch .unreadable, .unavailable {
            items = []
        }
        tabs.listen(on: cartChanges) { change => apply(change) }
    }

    /// Adds one unit of `item`, saves the cart and tells the other tabs.
    func add(_ item: CartItem) {
        apply(.added(item: item))
        save()
        tabs.post(on: cartChanges, message: .added(item: item))
    }

    /// Empties the cart, here and in the other tabs.
    func clear() {
        apply(.cleared)
        save()
        tabs.post(on: cartChanges, message: .cleared)
    }

    /// Applies a change made here or in another tab. Another tab has saved it
    /// already: local storage is shared between the tabs.
    private func apply(_ change: CartChange) {
        match change {
            case .added(let item) { items.append(item) }
            case .cleared { items = [] }
        }
    }

    private func save() {
        do {
            try storage.write(key: savedCart, value: items)
        } catch .quotaExceeded, .unavailable {
            // The cart still works for this visit, and in the other tabs.
        }
    }
}
