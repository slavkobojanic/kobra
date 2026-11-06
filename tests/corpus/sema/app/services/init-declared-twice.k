// A service declares at most one `init()`.
service Setup {
    @State var ready: Bool = false

    init() {
        ready = true
    }

    init() {
        ready = false
    }
}
