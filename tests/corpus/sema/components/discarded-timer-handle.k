import k.time.Clock

component Ticker {
    @Inject let clock: Clock
    @State var ticks: Int = 0

    @Effect(on: [])
    func start() {
        clock.every(1.second) { ticks += 1 }
    }

    @Effect(on: [])
    func startAndStop() {
        let timer = clock.every(1.second) { ticks += 1 }

        onCleanup { timer.cancel() }
    }

    view {
        text { "{ticks}" }
    }
}
