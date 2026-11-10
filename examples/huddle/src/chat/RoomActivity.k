import k.rpc.Broadcast

/// Fans what changes the lobby's counts out to every lobby open, one
/// instance per server process (with Postgres, every replica's lobbies).
/// Posts and deletions go to everyone; a person's reading goes to their
/// own tabs only.
@Scope(.server)
public service RoomActivity {
    private let events: Broadcast<String, LobbyEvent> = Broadcast()

    /// Tells every lobby. Inside a transaction it is held until the commit.
    func announce(event: LobbyEvent) {
        // No account is called "": names are 1 to 24 characters.
        events.publish(key: "", value: event)
    }

    /// Tells `member`'s lobbies only.
    func tell(member: String, event: LobbyEvent) {
        events.publish(key: member, value: event)
    }

    /// What `member`'s lobby hears from now on.
    func subscribe(member: String) -> Stream<LobbyEvent> {
        return Stream.merge(events.subscribe(key: ""), events.subscribe(key: member))
    }
}
