// A component with `@Load` fields needs a `loading` block, like a page.
func count() async -> Int {
    return 1
}

component Total {
    @Load let total: Int = await count()

    view {
        text { "{total}" }
    }
}
