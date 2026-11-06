// There are no zero-argument flag methods: `.dashed()` misses its value (K2003
// before `)`).
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
    view {
        Frame {
            "a"
        }
        .dashed()
    }
}
