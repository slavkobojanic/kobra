// A declaration with `@Load` fields gets a generated, synchronous `reload()`.
func count() async -> Int {
    return 1
}

page Stats {
    @Load let total: Int = await count()

    view {
        text { "{total}" }
        button { "Refresh" }
            .on(click: reload)
            //         ^^^^^^ resolves: member corpus.Stats.reload()
    }

    loading {
    }

    failed {
        button { "Retry" }
            .on(click: { reload() })
            //           ^^^^^^^^ type: Void
    }
}
