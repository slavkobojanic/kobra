// `view`, `failed` and `metadata` used as field and function names are ordinary identifiers.
component T {
    @Prop let view: View
    let metadata: Int = 1
    func failed() {}

    view {
        view
    }
}
