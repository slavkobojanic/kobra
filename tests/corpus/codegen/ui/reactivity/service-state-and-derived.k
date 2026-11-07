// A service's `@State` fields are signals and `@Derived` fields computed values, created in the
// constructor (codegen.md §13).
service Counter {
    @State var items: List<Int> = []
    @Derived let total: Int = items.sum()

    func add(value: Int) {
        items.append(value)
    }
}
