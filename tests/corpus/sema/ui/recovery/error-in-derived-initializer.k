// An unknown name in a `@Derived` initializer is K1201; the declared type is
// kept, so uses of the derived value check normally and cause nothing more.
component Total {
    @State var count: Int = 0
    @Derived let doubled: Int = cont * 2

    view {
        text { "{doubled + 1}" }
    }
}
