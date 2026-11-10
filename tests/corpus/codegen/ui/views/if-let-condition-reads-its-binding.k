// `if let count = unread, count > 0`: a condition after a `let` reads the
// unwrapped value, so the branch's test binds it first; it used to read
// `count` before declaring it (a ReferenceError in the browser,
// examples/huddle's unread counts).
component Badge {
    @State var unread: Int? = nil

    view {
        box {
            if let count = unread, count > 0 {
                text { "{count} unread" }
            }
        }
    }
}
