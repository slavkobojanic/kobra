/// A channel between the app's tabs, paired with the type of its messages:
/// `TabChannel<CartChange> = TabChannel(name: "cart")`
/// (`docs/design/45-web-platform.md`, "Messages between tabs"). A tab
/// can't post one type and another read a different one: channels with
/// the same name must carry the same message type.
@Native
public struct TabChannel<T> where T implements Codable {
    /// Creates the channel named `name`.
    ///
    /// - Parameter name: a string literal; channels with the same name are
    ///   the same channel.
    @Native
    @Literal
    init(name: String) {}

    /// The channel's name.
    var name: String
}
