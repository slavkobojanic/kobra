/// The fixture's page: write an entry, see the newest, find by topic.
page Home at "/" {
    @Inject let entries: Entries
    @Inject let topics: Topics

    @State var draft: String = ""
    @State var found: String = ""

    @Effect
    func load() async {
        await entries.refresh()
    }

    func write() async {
        let topic: String? = if draft.startsWith("#") { "tagged" } else { nil }
        await entries.write(draft, topic: topic)
        draft = ""
    }

    func find() async {
        let list = await topics.about("tagged")
        found = list.map { e => e.text }.joined(separator: ", ")
    }

    view {
        heading(level: 1) { "Notebook" }

        textField(value: bind draft)
            .label("Entry")

        button { "Write" }
            .on(click: write)

        paragraph { "{entries.count} entries" }
            .role(.status)

        list {
            for entry in entries.newest key entry.id {
                item { entry.text }
            }
        }

        if topics.isIndexed {
            button { "Find tagged" }
                .on(click: find)

            paragraph { "Found: {found}" }
        }

        if entries.problem != "" {
            paragraph { "Problem: {entries.problem}" }
                .role(.alert)
        }

        button { "Refresh" }
            .on(click: { await entries.refresh() })
    }
}
