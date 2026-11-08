// Each `await` of an async effect stops the run once a re-run or the unmount aborted it, the
// iterations of a `for await` included, so a superseded run never writes state
// (03-reactivity.md, "Async effects"; codegen.md §8).
service Feed {
    func load(id: Int) async -> Int {
        return id
    }
}

component Profile {
    @Prop let userId: Int
    @Prop let changes: Stream<Int>
    @Inject let feed: Feed
    @State var value: Int = 0

    @Effect
    func follow() async {
        value = await feed.load(id: userId)
        for await change in changes {
            value = change
        }
    }

    view {
        text { "{value}" }
    }
}
