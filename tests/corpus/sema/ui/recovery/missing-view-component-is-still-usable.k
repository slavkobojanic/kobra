// A component without a `view` is K3003 at its name; calls of it still check
// their arguments against its props and report nothing more.
component Badge {
    @Prop let text: String
}

component Home {
    view {
        Badge(text: "new")
    }
}
