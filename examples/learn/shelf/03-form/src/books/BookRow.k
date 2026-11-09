/// One book in the reading list: its title, and who wrote it.
public component BookRow {
    /// The book to show.
    @Prop let book: Book

    view {
        item {
            emphasis { book.title }

            text { " by {book.author}" }
        }
    }
}
