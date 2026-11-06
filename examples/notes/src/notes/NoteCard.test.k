import k.test.TestClock

test NoteCard {
    @Inject let clock: TestClock

    test "shows the title as a link, the start of the text and the tags" {
        let screen = render(NoteCard(note: summary(title: "Groceries", body: "Milk and eggs", tags: ["home", "weekly"], updatedAt: clock.now)))

        expect screen.link(named: "Groceries").isVisible
        expect screen.text("Milk and eggs").isVisible
        expect screen.list().items().map { item => item.text } == ["home", "weekly"]
    }

    test "says how long ago the note was edited" {
        let note = summary(title: "Groceries", updatedAt: clock.now)
        clock.advance(by: 5.minutes)

        let screen = render(NoteCard(note: note))

        expect screen.text("Edited 5 minutes ago").isVisible
    }

    test "a note without tags shows no tag list" {
        let screen = render(NoteCard(note: summary(title: "Groceries", updatedAt: clock.now)))

        expect not screen.list().exists
    }

    test "the pin toggle is pressed exactly when the note is pinned" {
        let pinned = render(NoteCard(note: summary(title: "Groceries", isPinned: true, updatedAt: clock.now)))
        let unpinned = render(NoteCard(note: summary(title: "Ideas", updatedAt: clock.now)))

        expect pinned.button(named: "Pin Groceries").isPressed
        expect not unpinned.button(named: "Pin Ideas").isPressed
    }

    test "pressing the pin toggle reports the note to the parent, once" {
        let note = summary(title: "Groceries", updatedAt: clock.now)
        var toggled: List<NoteSummary> = []
        let screen = render(NoteCard(note: note).on(pinToggled: { pressed => toggled.append(pressed) }))

        screen.button(named: "Pin Groceries").click()

        expect toggled == [note]
    }
}
