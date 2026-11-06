// Inside a view block, `loading`, `view`, `failed`, `page`, `key` and `metadata` are ordinary names (contextual keywords only act at member start).
component T {
    view {
        loading { "wait" }

        view { key }

        failed(reason)

        page

        metadata {}
    }
}
