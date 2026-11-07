// The generated `reload()` restarts the group; it is synchronous (14-async-data.md, "Reloading").
func count() async -> Int {
    return 1
}

page Stats {
    @Load let total: Int = await count()

    view {
        box {
            text { "{total}" }

            button { "Refresh" }
                .on(click: reload)
        }
    }

    loading {
    }

    failed {
        button { "Retry" }
            .on(click: { reload() })
    }
}
