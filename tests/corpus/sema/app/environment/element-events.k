// Every element has `appeared`, `disappeared` and `resized`, with
// `appearThreshold` and `appearMargin` (45-web-platform.md, "Element
// events").
import k.ui.ElementSize

service Feed {
    @State var pages: Int = 1

    func loadMore() async {
        pages += 1
    }
}

component Endless {
    @Inject let feed: Feed

    @State var width: ElementSize? = nil
    @State var visible: Bool = false

    func measure(_ size: ElementSize) {
        width = size
    }

    view {
        paragraph { "Loading more" }
            .appearMargin(400px)
            .appearThreshold(25%)
            .on(appeared: { await feed.loadMore() })

        box { "Chart" }
            .on(resized: measure)
            .on(appeared: { visible = true })
            .on(disappeared: { visible = false })

        button { "More" }
            .on(click: { await feed.loadMore() })
            .on(resized: { size => width = size })
    }
}
