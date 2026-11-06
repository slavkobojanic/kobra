// `@Load` fields are `let`.
func count() async -> Int {
    return 1
}

page Stats {
    @Load var total: Int = await count()

    view {
        text { "{total}" }
    }

    loading {
    }
}
