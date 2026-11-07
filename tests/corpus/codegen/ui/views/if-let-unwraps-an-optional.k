// `if let nickname` is a `when` on `!== null`; the branch reads the unwrapped value, so a change
// from one name to another updates the text without re-rendering the branch (runtime.md §5.3).
component Profile {
    @State var nickname: String? = nil

    view {
        box {
            if let nickname {
                text { nickname }
            }
        }
    }
}
