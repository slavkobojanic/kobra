// Each load run receives the group's `AbortSignal` as `$c` and passes it on, so an input change
// cancels the request (14-async-data.md, codegen.md §8).
service TickSource {
    func now() async -> Int {
        return 0
    }
}

page Ticks {
    @Inject let clock: TickSource

    @Load let tick: Int = await clock.now()

    view {
        text { "{tick}" }
    }

    loading {
    }
}
