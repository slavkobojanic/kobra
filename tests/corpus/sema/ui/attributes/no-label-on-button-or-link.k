// `.label` is not defined on `button` or `link`: their accessible name is their
// visible content (05-views.md). On a landmark it is the accessible name.
component Toolbar {
    view {
        button { "Delete" }
            .label("Delete item")
        link(url: Url("https://k.dev")) { "Docs" }
            .label("Documentation")
        nav { "menu" }
            .label("Main")
    }
}
