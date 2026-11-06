import k.validation.ValidationError

test NoteEditor {
    test "the form starts with the draft it was given" {
        let screen = render(EditorHost(draft: draft(title: "Plan", body: "Step one", tags: "work")))

        expect screen.textField(labeled: "Title").value == "Plan"
        expect screen.textArea(labeled: "Note").value == "Step one"
        expect screen.textField(labeled: "Tags").value == "work"
    }

    test "a valid draft is saved with exactly the values typed" {
        let screen = render(EditorHost(draft: draft(title: "")))

        screen.textField(labeled: "Title").fill("Groceries")
        screen.textArea(labeled: "Note").fill("Milk")
        screen.textField(labeled: "Tags").fill("home")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.text("Saved: Groceries | Milk | home").isVisible
    }

    test "a blank title is reported and nothing is saved" {
        let screen = render(EditorHost(draft: draft(title: "")))

        screen.textField(labeled: "Title").fill("   ")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.text("Give the note a title").isVisible
        expect screen.text("Nothing saved").isVisible
    }

    test "more than ten tags are reported under the tags field and nothing is saved" {
        let screen = render(EditorHost(draft: draft(title: "Plan")))

        screen.textField(labeled: "Tags").fill("a, b, c, d, e, f, g, h, i, j, k")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.text("Use at most 10 tags").isVisible
        expect screen.text("Nothing saved").isVisible
    }

    test "ten tags are fine, counting duplicates once" {
        let screen = render(EditorHost(draft: draft(title: "Plan")))

        screen.textField(labeled: "Tags").fill("a, b, c, d, e, f, g, h, i, j, A, b")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.text("Saved: Plan |  | a, b, c, d, e, f, g, h, i, j, A, b").isVisible
    }
}

/// Hosts a `NoteEditor` and shows what it last saved, as
/// `title | body | tags`, so tests can observe `save` through the screen.
component EditorHost {
    @Prop let draft: NoteDraft

    @State var saved: NoteDraft? = nil

    /// Matches `NoteEditor.save`. It never fails, so it never throws.
    func record(draft: NoteDraft) async throws ValidationError {
        saved = draft
    }

    view {
        NoteEditor(draft: draft, save: record)

        if let saved {
            paragraph { "Saved: {saved.title} | {saved.body} | {saved.tags}" }
        } else {
            paragraph { "Nothing saved" }
        }
    }
}
