// A service's `init()` takes no parameters: its inputs are its dependencies.
service Limits {
    @State var limit: Int = 0

    init(start: Int) {
        limit = start
    }
}
