// Writing a `@Binding` or `@State` field is allowed, directly, compounded and
// through nested fields.
struct Draft {
    var title: String
}

component Editor {
    @Binding var count: Int
    @State var draft: Draft = Draft(title: "")
    @State var history: List<String> = []

    func edit() {
        count += 1
        draft.title = "New"
        history.append(draft.title)
    }

    view {
        text { "{count} {draft.title}" }
    }
}
