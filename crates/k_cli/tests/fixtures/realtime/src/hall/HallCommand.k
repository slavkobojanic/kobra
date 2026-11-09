/// What a person says on a hall's channel.
public enum HallCommand implements Codable, Equatable {
    /// Shows this channel's person with `status`.
    case status(text: String)
    /// Leaves the hall's presence while keeping the channel open.
    case leave
}
