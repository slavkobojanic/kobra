// A service's own methods and `init()` write its `@State` and plain `var` fields.
service Cart {
    @State var items: List<Int> = []
    var retries: Int = 0

    init() {
        retries = 0
    }

    func add(item: Int) {
        items.append(item)
        retries += 1
    }
}
