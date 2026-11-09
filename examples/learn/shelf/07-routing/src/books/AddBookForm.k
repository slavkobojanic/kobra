import k.validation.Form

/// A form for adding a book to the shelf. It checks what was typed, saves a
/// book that passes the checks on the server, reports it with `added`, and
/// then clears its fields for the next one.
public component AddBookForm {
    @Inject let books: BookService
    @Form var draft: BookDraft = .empty
    @State var problem: String? = nil

    /// Fired with the saved book, once the server has stored it.
    @Emits let added: Event<Book>

    func submit() async {
        problem = nil

        do {
            let book = try await books.add(draft: draft.value)

            added.emit(book)
            draft.reset()
        } catch .rpcFailure {
            problem = "Couldn't save the book. Check your connection and try again."
        }
    }

    view {
        form {
            textField(value: bind draft.title)
                .label("Title")
                .class(.field)

            textField(value: bind draft.author)
                .label("Author")
                .class(.field)

            if let problem {
                paragraph { problem }
                    .role(.alert)
            }

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
