// An optional `@Slot` (`View? = nil`) that the parent doesn't fill is
// `nil` in the child, so `if let toolbar { … }` skips it.
component Card {
    @Slot let content: View
    @Slot let toolbar: View? = nil

    view {
        if let toolbar {
            toolbar
        }
        content
    }
}

component Page1 {
    view {
        Card {
            text { "Hello" }
        }
    }
}
