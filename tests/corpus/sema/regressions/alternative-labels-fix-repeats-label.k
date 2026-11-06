// `link(to:)` and `link(url:)` are alternatives: passing both is K2004 at the
// second label. `link` does have a `url:` argument, so the message must not say it
// doesn't, and the closest-label fix must not replace `url` with `url`.
page About {
    view {
        text { "About" }
    }
}

page Home {
    view {
        link(to: .about, url: Url("https://k.dev")) { "Both" }
    }
}
