// Links on a component and on its props resolve like on any declaration.

/// A label; its [`text`] is required. See [`Card.title`].
component Badge {
    /// Shown inside the badge, like a [`Card`]'s title.
    @Prop let text: String

    view {
        paragraph { text }
    }
}

/// A card.
struct Card {
    let title: String
}
