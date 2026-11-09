import k.rpc.Broadcast

/// Fans a room's events out to every channel open on it, one instance per
/// server process.
@Scope(.server)
public service RoomEvents {
    private let events: Broadcast<String, ChatEvent> = Broadcast()

    /// Sends `event` to every channel of `room`. Inside a transaction it is
    /// held until the commit.
    func publish(room: String, event: ChatEvent) {
        events.publish(key: room, value: event)
    }

    /// The events of `room` from now on.
    func subscribe(room: String) -> Stream<ChatEvent> {
        return events.subscribe(key: room)
    }
}
