import k.validation.Form

/// The title, text and tags form, used both to create and to edit a note.
///
/// It validates against the constraints on `NoteDraft` as the person types,
/// and again on submit. `save` runs only with a valid draft, and the submit
/// button is disabled while it runs, so a double click can't save twice.
public component NoteEditor {
    /// The values the form starts with.
    @Prop let draft: NoteDraft

    /// Stores a valid draft. The parent handles failures such as conflicts,
    /// because only it knows what to do next.
    @Prop let save: (NoteDraft) async -> Void

    /// The submit button's text.
    @Prop let submitLabel: String = "Save"

    @Form var input: NoteDraft = draft

    func submit() async {
        await save(input.value)
    }

    view {
        form {
            textField(value: bind input.title)
                .label("Title")

            textArea(value: bind input.body)
                .label("Note")
                .rows(12)

            textField(value: bind input.tags)
                .label("Tags")
                .placeholder("work, ideas")

            button { submitLabel }
                .type(.submit)
                .disabled(input.isSubmitting)
        }
        .class(.editor)
        .on(submit: input.handle(submit))
    }

    style {
        .editor { display: grid; gap: .md; }
    }
}
