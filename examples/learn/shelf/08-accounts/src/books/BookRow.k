/// One book in the reading list: its title, which links to the book's
/// page, and who wrote it.
public component BookRow {
    /// The book to show.
    @Prop let book: Book

    view {
        item {
            link(to: .bookDetails(id: book.id)) { book.title }

            text { " by {book.author}" }
                .class(.author)
        }
        .class(.row)
    }

    style {
        .row {
            padding: .md;
            borderBottom: 1px solid .border;

            &:last-child { borderBottom: none; }
        }

        .author { color: .muted; }
    }
}
