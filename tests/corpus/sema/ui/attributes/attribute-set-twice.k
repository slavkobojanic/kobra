// Each attribute is set once (K3104 at the second, label at the first).
component Pay {
    @State var a: Bool = false
    @State var b: Bool = false

    view {
        button { "Pay" }
            .disabled(a)
            .type(.submit)
            .disabled(b)
    }
}
