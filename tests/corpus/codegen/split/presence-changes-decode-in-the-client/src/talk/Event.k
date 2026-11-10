import k.rpc.PresenceChange

public enum Event implements Codable {
    case said(text: String)
    /// A stdlib type reached only from this codec: the client bundle needs
    /// its codec too (examples/huddle's "In this room" never updated in a
    /// browser: every presence event was skipped).
    case presence(PresenceChange<Who>)
}
