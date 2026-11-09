// `TabMessaging` (45-web-platform.md, "Messages between tabs"): a channel's message type is the
// one `post`, `listen` and `messages` use, so a cart change can't be posted on a channel of counts.
// An `.app` service listens in `init()` and may drop the subscription; an effect follows
// `messages` with `for await`; `listen`'s subscription cancels it.
import k.device.Subscription
import k.device.TabChannel
import k.device.TabMessaging

enum CartChange implements Codable {
    case added(id: Int)
    case cleared
}

let cartChanges: TabChannel<CartChange> = TabChannel(name: "cart")
let counts: TabChannel<Int> = TabChannel(name: "counts")

service CartService {
    @Inject let tabs: TabMessaging

    @State var ids: List<Int> = []
    @State var heard: Int = 0

    init() {
        tabs.listen(on: cartChanges) { change => apply(change) }
    }

    func apply(_ change: CartChange) {
        match change {
            case .added(let id) { ids.append(id) }
            case .cleared { ids = [] }
        }
    }

    func add(_ id: Int) {
        ids.append(id)
        tabs.post(on: cartChanges, message: .added(id: id))
        tabs.post(on: counts, message: ids.count)
    }

    func wrong() {
        tabs.post(on: counts, message: CartChange.cleared)
    }

    func follow() async {
        for await count in tabs.messages(on: counts) {
            heard = count
        }
    }

    func stop(_ subscription: Subscription) {
        subscription.cancel()
    }
}
