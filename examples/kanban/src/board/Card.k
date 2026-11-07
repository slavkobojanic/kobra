/// One piece of work on a board.
///
/// A value: moving a card between columns moves a copy, and the board keeps
/// exactly one copy of each card.
public struct Card implements Codable, Equatable {
    /// Identifies the card. Never changes.
    let id: CardId

    /// What the card is about. Trimmed, never blank, at most
    /// `CardDraft`'s limit long.
    var title: String

    /// Longer notes. May be empty.
    var notes: String

    /// How urgent the card is.
    var priority: Priority
}
