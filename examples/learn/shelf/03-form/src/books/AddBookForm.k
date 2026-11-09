import k.validation.Form

/// A form for adding a book to the list. It checks what was typed,
/// reports a book that passes the checks with `added`, and then clears its
/// fields for the next one.
public component AddBookForm {
    @Form var draft: BookDraft = .empty

    /// Fired with the checked title and author when the form is submitted.
    @Emits let added: Event<BookDraft>

    func submit() {
        added.emit(draft.value)
        draft.reset()
    }

    view {
        form {
            textField(value: bind draft.title)
                .label("Title")

            textField(value: bind draft.author)
                .label("Author")

            button { "Add book" }
                .type(.submit)
        }
        .on(submit: draft.handle(submit))
    }
}
