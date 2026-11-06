// `@Effect` applies to functions of services as well as components.
service Sync {
    @State var draft: String = ""
    @State var saves: Int = 0

    @Effect
    func save() {
        let text = draft
        saves += 1
    }
}
