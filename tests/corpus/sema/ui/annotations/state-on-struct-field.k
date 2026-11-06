// Reactive annotations don't apply to plain data: `@State` and `@Derived` on a
// struct's fields are K3002.
struct Point {
    @State var x: Int
    @Derived let y: Int
}
