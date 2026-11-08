// A function value a component hands to a service can outlive the component: the service may
// keep it (a listener) or call it later (a `Clock` timer's action). So a lambda or a method of the
// component passed to a service's function parameter is bound to the component's lifetime:
// `$k.bound(f, $life, mode)`, which runs nothing once the component is disposed, and which the
// browser `Clock` stops a timer by (03-reactivity.md, "What a component leaves behind").
import k.time.Clock

service Bus {
    var listeners: List<() async -> Void> = []
    var plain: List<() -> Void> = []

    func listen(_ f: () async -> Void) {
        listeners.append(f)
    }

    func listenPlain(_ f: () -> Void) {
        plain.append(f)
    }

    func ask(_ f: () -> Int) -> Int {
        return f()
    }
}

component Badge {
    @Inject let bus: Bus
    @Inject let clock: Clock
    @State var count: Int = 0

    func heard() async {
        count += 1
    }

    @Effect
    func register() {
        bus.listen({ count += 1 })
        bus.listen(heard)
        bus.listenPlain({ count += 1 })
        let n = bus.ask({ 7 })
    }

    view {
        button { "Remind" }
            .on(click: {
                let timer = clock.after(1.seconds) { count += 1 }
            })
    }
}
