// A slot holds `View`, `View?` or a function returning `View`; anything else is
// K3005 at the type.
component Card {
    @Slot let title: String
    @Slot let row: (Int) -> Int
    @Slot let content: View

    view {
        box {
            content
        }
    }
}
