/// One line of the call's chat, sent on the `chatLines` data channel.
public struct ChatLine implements Codable, Equatable {
    /// Who wrote it.
    let from: String
    /// What they wrote.
    let text: String
}
