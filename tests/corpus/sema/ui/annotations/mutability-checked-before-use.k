// A wrongly declared `@Prop var` is reported once (K3001). Writing it later is not
// reported again as a write to a prop: the fix for the declaration decides.
component Title {
    @Prop var text: String

    func shout() {
        text = text + "!"
    }

    view {
        paragraph { text }
    }
}
