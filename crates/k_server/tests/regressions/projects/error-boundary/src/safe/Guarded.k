import k.ui.ErrorBoundary

/// Renders the item at `index` inside an `ErrorBoundary`: an index out of
/// range panics, and the boundary shows its fallback instead.
public component Guarded {
    @Prop let index: Int
    @State var items: List<String> = ["a"]

    view {
        box {
            ErrorBoundary {
                text { items[index] }
            }
            .fallback {
                text { "fallback" }
            }
        }
    }
}
