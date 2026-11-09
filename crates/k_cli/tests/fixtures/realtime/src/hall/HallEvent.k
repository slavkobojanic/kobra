import k.rpc.PresenceChange

/// What the server says on a hall's channel.
public enum HallEvent implements Codable, Equatable {
    /// Who was present when the channel opened, sent first.
    case snapshot(present: List<Member>)
    /// Someone arrived, left or changed their status.
    case presence(PresenceChange<Member>)
}
