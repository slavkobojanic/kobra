// Compound assignment to a prop is a write too (K3014 at the place).
component Counter {
    @Prop let count: Int

    func increment() {
        count += 1
    }

    view {
        text { "{count}" }
    }
}
