// The client factory and the server renderer are printed from one lowered view: the template the
// client claims is exactly the markup the server writes around the dynamic parts
// (codegen.md §11.3).
component Row {
    @Prop let title: String
    @Prop let isDone: Bool

    view {
        item {
            text { title }

            if isDone {
                text { "done" }
            }
        }
    }
}
