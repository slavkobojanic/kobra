// The options are typed, `resized` passes an `ElementSize`, and no other
// observer event exists.
component Wrong {
    @State var count: Int = 0

    view {
        paragraph { "a" }
            .appearMargin("400px")
        paragraph { "b" }
            .appearThreshold(0.5)
        paragraph { "c" }
            .on(resized: { (size: Int) => count = size })
        paragraph { "d" }
            .on(scrolled: { count += 1 })
    }
}
