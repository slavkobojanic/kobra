import k.ui.Grid
import k.ui.Spacer
import k.ui.Stack
import k.ui.VisuallyHidden

/// Uses the stdlib's layout components (05-views.md: "The stdlib provides
/// layout components (`Stack`, `Grid`, `Spacer`)").
public component Layout {
    view {
        Stack {
            text { "one" }
            Spacer()
            Grid(columns: 3) {
                text { "two" }
            }
            VisuallyHidden {
                text { "three" }
            }
        }
    }
}
