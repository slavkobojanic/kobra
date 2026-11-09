import k.ui.ElementSize

public page Home at "/" {
    @State var pages: Int = 1
    @State var size: ElementSize? = nil

    view {
        text { "Page {pages}" }
            .appearMargin(400px)
            .appearThreshold(50%)
            .on(appeared: { pages += 1 })
        box { "Chart" }
            .on(resized: { s => size = s })
    }
}
