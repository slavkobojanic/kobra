import k.rpc.Broadcast

/// The process-wide fan-out of ticks.
@Scope(.server)
public service Ticker {
    private let ticks: Broadcast<Int, Tick> = Broadcast()
    private let counts: Broadcast<Int, Int> = Broadcast()

    func subscribeTicks() -> Stream<Tick> {
        return ticks.subscribe(key: 0)
    }

    func subscribeCounts() -> Stream<Int> {
        return counts.subscribe(key: 0)
    }
}
