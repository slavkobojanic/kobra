// `image` defines `.loading(ImageLoading)`.
component Logo {
    view {
        image(source: Url("https://k.dev/logo.png"), description: "K")
            .loading(.lazy)
    }
}
