/// One book in the reading list: its title, and who wrote it.
public component BookRow {
    /// The book to show.
    @Prop let book: Book

    view {
        item {
            emphasis { book.title }

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
