// A `@Load` run stops after its `await` once the group aborted it (an input change, `reload()`,
// the unmount), like an effect's run (14-async-data.md, codegen.md §8).
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
