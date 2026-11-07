// A `@Derived` value that reads another `@Derived` value reads it through `get()`, so both stay
// lazy and the chain is tracked (runtime.md §3.2).
component Prices {
    @State var cents: Int = 250
    @Derived let euros: Int = cents / 100
    @Derived let label: String = "{euros} EUR"

    view {
        text { label }
    }
}
