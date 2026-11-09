import k.device.TabChannel
import k.device.TabMessaging

let cartChanges: TabChannel<Int> = TabChannel(name: "cart")

public service CartService {
    @Inject let tabs: TabMessaging

    @State var count: Int = 0

    init() {
        tabs.listen(on: cartChanges) { n => count = n }
    }

    func add() {
        count += 1
        tabs.post(on: cartChanges, message: count)
    }
}
