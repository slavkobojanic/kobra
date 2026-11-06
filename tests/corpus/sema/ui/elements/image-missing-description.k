// `image` requires `description` (the text alternative): K3102 at the insertion
// point before `)`.
component Logo {
    view {
        image(source: Url("https://k.dev/logo.png"))
        image(source: Url("https://k.dev/logo.png"), description: "K")
    }
}
