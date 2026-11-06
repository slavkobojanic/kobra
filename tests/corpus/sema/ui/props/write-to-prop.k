// A child writing to a `@Prop` is K3014 with a fix to make it a `@Binding`
// (04-components.md, "Two-way binding").
component Title {
    @Prop let title: String

    func reset() {
        title = ""
    }

    view {
        paragraph { title }
    }
}
