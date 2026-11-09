// Elements with required named arguments: `image(source:description:)`,
// `link(url:)`, `heading(level:)`, inputs with their bound value.
component Gallery {
    @State var query: String = ""
    @State var isShown: Bool = false
    @State var count: Int = 0

    view {
        image(source: Url("https://k.dev/logo.png"), description: "The K logo")
        //                                           ^^^^^^^^^^^ resolves: attribute image.description
        link(url: Url("https://k.dev")) { "Docs" }
        heading(level: 1) { "Gallery" }
        textField(value: bind query)
        passwordField(value: bind query)
        textArea(value: bind query)
        numberField(value: bind count)
        checkbox(isChecked: bind isShown)
        audio(source: .url(Url("https://k.dev/a.mp3")))
        video(source: .url(Url("https://k.dev/v.mp4")))
            .captions(.notNeeded)
    }
}
