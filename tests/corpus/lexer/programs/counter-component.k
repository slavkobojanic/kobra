component Counter {
    @State var count: Int = 0

    view {
        button("Add") { count += 1 }
        paragraph { "Count: {count}" }
    }

    style {
        .count { fontSize: 2rem; }
    }
}
