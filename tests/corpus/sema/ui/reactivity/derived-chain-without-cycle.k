// A chain of `@Derived` fields that reads in one direction is fine,
// whatever the declaration order.
component Prices {
    @State var base: Int = 10
    @Derived let total: Int = taxed + 1
    @Derived let taxed: Int = base * 2

    view {
        text { "{total}" }
    }
}
