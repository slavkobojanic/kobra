// `bind` where no binding is expected (a `@Prop`, or an element argument that
// isn't a `Binding`) is K3018 with a fix that removes `bind `.
component Label {
    @Prop let text: String

    view {
        paragraph { text }
    }
}

component Home {
    @State var title: String = ""

    view {
        Label(text: bind title)
        image(source: Url("https://k.dev/a.png"), description: bind title)
    }
}
