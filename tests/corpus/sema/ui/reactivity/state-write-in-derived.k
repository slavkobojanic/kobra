// Writing `@State` while computing a `@Derived` value is K3011 at the assignment
// (03-reactivity.md).
component Total {
    @State var count: Int = 0
    @State var reads: Int = 0
    @Derived let label: String = if count > 0 {
        reads += 1
        "some"
    } else {
        "none"
    }

    view {
        text { label }
    }
}
