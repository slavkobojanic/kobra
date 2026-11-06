// A `Bool` prop with a default is chained with its value: `.dashed(true)` and
// `.dashed(isBusy)` (01-principles.md: no zero-argument flag methods).
component Frame {
    @Slot let content: View
    @Prop let dashed: Bool = false

    view {
        box {
            content
        }
    }
}

component Home {
    @State var isBusy: Bool = false

    view {
        Frame {
            "a"
        }
        .dashed(true)

        Frame {
            "b"
        }
        .dashed(isBusy)
    }
}
