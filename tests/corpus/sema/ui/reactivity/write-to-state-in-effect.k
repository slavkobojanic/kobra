// An effect may write state: only `@Derived` initializers may not.
component Mirror {
    @Prop let source: Int
    @State var copy: Int = 0

    @Effect
    func sync() {
        copy = source
    }

    view {
        text { "{copy}" }
    }
}
