// A `@Derived` value without a written type takes its initializer's type.
component Total {
    @State var count: Int = 0
    @Derived let doubled = count * 2
    //           ^^^^^^^ type: Int
    @Derived let label = if count > 0 { "some" } else { "none" }
    //           ^^^^^ type: String

    view {
        text { "{doubled} {label}" }
    }
}
