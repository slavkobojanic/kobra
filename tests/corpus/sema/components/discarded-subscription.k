// A `Subscription` keeps its callbacks running until it is cancelled (45-web-platform.md,
// "Ownership and lifetime"), so discarding one inside an effect is the same warning as discarding a
// `TimerHandle`: the callbacks would outlive the component.
import k.device.Subscription

component Feed {
    @Prop let subscription: Subscription

    func renew() -> Subscription {
        return subscription
    }

    @Effect
    func start() {
        renew()
    }

    @Effect
    func keep() {
        let kept = renew()
        onCleanup {
            kept.cancel()
        }
    }

    view {
        text { "Feed" }
    }
}
