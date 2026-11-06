// HTML attribute spellings are not labels: `src:` is K2004 with the closest
// label `source` as a fix; the `source` it stands for is not reported missing.
component Logo {
    view {
        image(src: Url("https://k.dev/logo.png"), description: "K")
    }
}
