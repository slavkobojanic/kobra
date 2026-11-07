// An `@Effect` is a user effect on the client and never runs on the server (03-reactivity.md,
// "Where effects run"; codegen.md §10).
import k.observe.Log

component Logger {
    @State var count: Int = 0

    @Effect
    func logChanges() {
        Log.info("count is now {count}")
    }

    view {
        text { "{count}" }
    }
}
