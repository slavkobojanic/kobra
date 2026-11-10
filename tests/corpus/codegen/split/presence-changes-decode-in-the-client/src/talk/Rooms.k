import k.rpc.Broadcast

@Scope(.server)
public service Rooms {
    private let events: Broadcast<String, Event> = Broadcast()

    func subscribe(room: String) -> Stream<Event> {
        return events.subscribe(key: room)
    }

    func publish(room: String, event: Event) {
        events.publish(key: room, value: event)
    }
}
