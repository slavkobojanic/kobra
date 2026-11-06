// `link` has two alternative forms, `link(to:)` and `link(url:)`; with neither it
// is K3102, and the alternatives are listed.
component Nav {
    view {
        link { "Home" }
        link(url: Url("https://k.dev")) { "Docs" }
    }
}
