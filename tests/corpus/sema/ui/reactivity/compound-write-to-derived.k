// Compound assignment to a `@Derived` value is a write too (K3010).
component Total {
    @State var count: Int = 0
    @Derived let doubled: Int = count * 2

    func bump() {
        doubled += 1
    }

    view {
        text { "{doubled}" }
    }
}
