// `@Effect(on: [])` runs once after mount and never re-runs: the whole body is untracked
// (03-reactivity.md).
import k.observe.Log

component Banner {
    @State var seen: Int = 0

    @Effect(on: [])
    func mounted() {
        Log.info("mounted with {seen}")
    }

    view {
        text { "{seen}" }
    }
}
