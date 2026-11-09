import k.rpc.Broadcast

/// The process-wide fan-out of room events.
@Scope(.server)
public service RoomEvents {
    private let events: Broadcast<String, ChatEvent> = Broadcast()

    func publish(room: String, event: ChatEvent) {
        events.publish(key: room, value: event)
    }

    func subscribe(room: String) -> Stream<ChatEvent> {
        return events.subscribe(key: room)
    }
}
