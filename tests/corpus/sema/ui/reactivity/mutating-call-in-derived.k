// A mutating call on state inside a `@Derived` initializer is a state write
// (K3011 at the call); a non-mutating `appending` is fine.
component Log1 {
    @State var lines: List<String> = []
    @Derived let more: List<String> = lines.appending("x")
    @Derived let count: Int = if lines.isEmpty {
        0
    } else {
        lines.append("y")
        1
    }

    view {
        text { "{count} {more.count}" }
    }
}
