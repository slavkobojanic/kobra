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
                .class(.field)

            textField(value: bind draft.author)
                .label("Author")
                .class(.field)

            button { "Add book" }
                .type(.submit)
                .class(.add)
        }
        .class(.form)
        .on(submit: draft.handle(submit))
    }

    style {
        .form {
            display: grid;
            gap: .sm;
        }

        .field {
            padding: .sm;
            border: 1px solid .border;
            borderRadius: .md;
            backgroundColor: .surface;
            color: .text;
        }

        .add {
            justifySelf: start;
            marginTop: .sm;
            padding: .sm .md;
            border: none;
            borderRadius: .md;
            backgroundColor: .primary;
            color: .onPrimary;
        }
    }
}
