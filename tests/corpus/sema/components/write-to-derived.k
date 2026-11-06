component Total {
    @State var count: Int = 0
    @Derived let doubled: Int = count * 2

    func reset() {
        count = 0
        doubled = 0
    }

    view {
        text { "{doubled}" }
    }
}
