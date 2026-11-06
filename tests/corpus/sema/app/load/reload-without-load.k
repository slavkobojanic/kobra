// Without `@Load` fields there is no generated `reload()`.
page Plain {
    view {
        button { "Reload" }
            .on(click: { reload() })
    }
}
