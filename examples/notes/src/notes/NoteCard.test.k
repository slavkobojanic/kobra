test NoteCard {
    test "shows the title as a link, the start of the text and the tags" {
        let screen = render(NoteCard(note: summary(title: "Groceries", body: "Milk and eggs", tags: ["home", "weekly"])))

        expect screen.link(named: "Groceries").isVisible
        expect screen.text("Milk and eggs").isVisible
        expect screen.list().items().map { item => item.text } == ["home", "weekly"]
    }

    test "a note without tags shows no tag list" {
        let screen = render(NoteCard(note: summary(title: "Groceries")))

        expect screen.list().exists == false
    }

    test "the pin toggle is pressed exactly when the note is pinned" {
        let pinned = render(NoteCard(note: summary(title: "Groceries", isPinned: true)))
        let unpinned = render(NoteCard(note: summary(title: "Ideas")))

        expect pinned.button(named: "Pin").isPressed
        expect unpinned.button(named: "Pin").isPressed == false
    }

    test "pressing the pin toggle reports the note to the parent, once" {
        let note = summary(title: "Groceries")
        var toggled: List<NoteSummary> = []
        let screen = render(NoteCard(note: note).on(pinToggled: { pressed => toggled.append(pressed) }))

        screen.button(named: "Pin").click()

        expect toggled == [note]
    }
}
