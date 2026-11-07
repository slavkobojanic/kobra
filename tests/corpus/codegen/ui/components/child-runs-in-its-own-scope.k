// Each component instance runs inside `$k.scope`, so unmounting disposes its effects
// (codegen.md §10, runtime.md §3.5). The child is placed between block markers.
component Badge {
    @Prop let label: String

    view {
        text { label }
    }
}

component Header {
    view {
        box {
            Badge(label: "new")
        }
    }
}
