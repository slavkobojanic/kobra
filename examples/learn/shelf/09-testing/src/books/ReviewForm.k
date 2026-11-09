import k.validation.Form

/// The notes and rating of one book, and a button that saves them.
public component ReviewForm {
    @Inject let books: BookService

    /// The book to review, as it was loaded.
    @Prop let book: Book

    @Form var review: BookReview = .empty(notes: book.notes, rating: book.rating)
    @State var status: String? = nil

    func submit() async {
        status = nil

        do {
            try await books.review(book: book, review: review.value)

            status = "Saved."
        } catch .notFound {
            status = "This book isn't on the shelf any more."
        } catch .rpcFailure {
            status = "Couldn't save. Check your connection and try again."
        }
    }

    view {
        form {
            dropdown(selection: bind review.rating) {
                option(value: 0) { "Not rated" }

                for stars in [1, 2, 3, 4, 5] key stars {
                    option(value: stars) { "{stars} of 5" }
                }
            }
            .label("Rating")
            .class(.field)

            textArea(value: bind review.notes)
                .label("Notes")
                .rows(6)
                .class(.field)

            if let status {
                paragraph { status }
                    .role(.status)
            }

            button { "Save" }
                .type(.submit)
                .class(.save)
        }
        .class(.form)
        .on(submit: review.handle(submit))
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

        .save {
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
