/// A data channel paired with the type of its messages
/// (`docs/design/46-media-and-rtc.md`, "Typed data channels"):
/// `let chatLines: DataChannelKind<ChatLine> = DataChannelKind(name: "chat")`.
///
/// Both peers open the channel of a kind independently, in any order: its
/// id is derived from the name. Kinds with the same name are the same
/// channel and must carry the same type (K2408), and two names whose ids
/// collide are K2409.
@Native
public struct DataChannelKind<T> where T implements Codable {
    /// Creates the kind named `name`.
    ///
    /// - Parameter name: a string literal.
    @Native
    @Literal
    init(name: String) {}

    /// The channel's name.
    var name: String
}
