// `@Derived` is a lazy computed value on the client and is computed once, as a plain local, in the
// server renderer (codegen.md §10, §11.2).
component Doubler {
    @State var count: Int = 1
    @Derived let doubled: Int = count * 2

    view {
        text { "{doubled}" }
    }
}
