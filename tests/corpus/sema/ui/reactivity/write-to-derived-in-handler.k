// A write to a `@Derived` field in a view lambda is K3010, with a label at the
// field.
component Total {
    @State var count: Int = 0
    @Derived let doubled: Int = count * 2

    view {
        button { "Reset" }
            .on(click: { doubled = 0 })
    }
}
