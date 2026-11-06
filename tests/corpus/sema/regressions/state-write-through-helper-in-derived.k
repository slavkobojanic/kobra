// Writing `@State` inside a `@Derived` computation is an error (03-reactivity.md),
// also when the write happens in a method the initializer calls: K3011 at the call.
component Counter {
    @State var count: Int = 0
    @Derived let doubled: Int = bump()

    func bump() -> Int {
        count = count + 1
        return count * 2
    }

    view {
        text { "{doubled}" }
    }
}
