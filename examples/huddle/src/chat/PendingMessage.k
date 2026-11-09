import k.crypto.Uuid

/// A message this tab sent that the room hasn't echoed yet.
public struct PendingMessage implements Equatable {
    /// Sent with the message; the echo carries it back.
    let clientId: Uuid
    let text: String
    /// `true` once it couldn't be delivered; the page offers to retry.
    let failed: Bool = false
}
