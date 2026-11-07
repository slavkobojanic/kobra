// `.disabled` and the universal `.hidden` are boolean HTML attributes: present for `true`, absent
// for `false`; a reactive value is an attribute binding (runtime.md §5.2).
component Submit {
    @State var isBusy: Bool = false
    @State var isHidden: Bool = false

    view {
        box {
            button { "Locked" }
                .disabled(true)

            button { "Open" }
                .disabled(false)

            button { "Send" }
                .disabled(isBusy)

            paragraph { "Saved" }
                .hidden(isHidden)
        }
    }
}
