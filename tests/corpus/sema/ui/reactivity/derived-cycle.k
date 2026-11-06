// A `@Derived` field can't depend on itself, directly or through other
// `@Derived` fields (03-reactivity.md): no value could be computed first.
// K3026 at each field of the cycle; `ok` reads only state.
component Counter {
    @State var n: Int = 0
    @Derived let a: Int = b + 1
    @Derived let b: Int = a + 1
    @Derived let c: Int = c
    @Derived let ok: Int = n * 2
    @Derived let viaSelf: Int = self.viaSelf + ok

    view {
        text { "{a} {b} {c} {ok} {viaSelf}" }
    }
}
