// The server renderer has no signals, effects or DOM: state is plain locals and effects don't exist
// there (codegen.md §11.2).
import k.observe.Log

component Ticker {
    @State var ticks: Int = 0
    @Derived let label: String = "{ticks} ticks"

    @Effect
    func report() {
        Log.info(label)
    }

    view {
        text { label }
    }
}
