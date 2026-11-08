// Kinds with the same name are the same kind (41-drag-and-drop.md), so two kinds named alike must
// carry the same payload type: a drop of one would deliver the other's payload. A kind named like an
// earlier one (in file path order) with another payload type is K2406 at its name literal, with a
// label at the earlier one. Redeclaring a kind with the same payload type is fine.

/// A card on the board.
public struct Card {
    /// Its id.
    let id: Int
}
