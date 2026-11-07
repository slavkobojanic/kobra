import k.crypto.Uuid

/// Identifies a card, wherever it moves.
public struct CardId implements Codable, Equatable, Hashable {
    /// The random UUID behind this id.
    let value: Uuid

    /// The id as text: the card's key in its column's list.
    var text: String {
        return "{value}"
    }
}
