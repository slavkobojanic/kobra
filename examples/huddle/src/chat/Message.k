import k.crypto.Uuid

/// A message, stored in the `message` table. Server-only, like every
/// entity; clients receive a `ChatMessage`.
public entity Message {
    @Id let id: Uuid

    /// The room it was posted in.
    @Index let room: String

    /// Its place in the room: 1 for the first message, then one more each
    /// time. Clients order by it, so every tab shows the same order.
    let number: Int

    /// Who posted it.
    let author: String

    let text: String

    /// When it was posted, from `ChatApi`'s clock.
    let sentAt: Date
}
