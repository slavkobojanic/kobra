import k.device.TabChannel
import k.device.TabMessaging

let cartChanges: TabChannel<CartChange> = TabChannel(name: "cart")

/// The cart, kept in step with the app's other tabs.
public service CartService {
    @Inject let tabs: TabMessaging

    @State var count: Int = 0
    @State var last: String = "none"

    init() {
        // An app-scoped service lives as long as the tab, so its
        // subscription may too.
        tabs.listen(on: cartChanges) { change => apply(change) }
    }

    func add(name: String, quantity: Int) {
        let change = CartChange.added(name: name, quantity: quantity)
        apply(change)
        tabs.post(on: cartChanges, message: change)
    }

    func clear() {
        apply(.cleared)
        tabs.post(on: cartChanges, message: .cleared)
    }

    func apply(_ change: CartChange) {
        match change {
            case .added(let name, let quantity) {
                count += quantity
                last = name
            }
            case .cleared {
                count = 0
                last = "none"
            }
        }
    }
}
