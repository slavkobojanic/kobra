public page Home at "/" {
    @Inject let notes: Notes

    view {
        button { "Import" }
            .on(click: notes.load)

        button { "Export" }
            .on(click: notes.export)
    }
}
