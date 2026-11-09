import k.ui.ElementSize

/// A measurement CSS can't use: how many columns of 100px fit.
public component Measured {
    @State var size: ElementSize? = nil
    @State var resizes: Int = 0

    func measure(_ newSize: ElementSize) {
        size = newSize
        resizes += 1
    }

    func record(_ newSize: ElementSize) async {
        size = newSize
    }

    view {
        text { "Measured area" }
            .on(resized: measure)

        text { "Async area" }
            .on(resized: { s => await record(s) })

        if size == ElementSize(width: 320px, height: 200px) {
            text { "Three columns" }
        } else if size == ElementSize(width: 100px, height: 50px) {
            text { "One column" }
        } else {
            text { "Not measured" }
        }

        text { "Resized {resizes} times" }
    }
}
