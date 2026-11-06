// A component can place its children but not inspect them: `View` has no members
// (K1203).
component Card {
    @Slot let content: View

    view {
        box {
            content
        }
        text { "{content.children}" }
    }
}
