import k.rpc.Broadcast

/// The process-wide fan-out of messages by room.
@Scope(.server)
public service RoomChannel {
    private let channel: Broadcast<String, Message> = Broadcast()

    func publish(message: Message) {
        channel.publish(key: message.room, value: message)
    }

    func subscribe(room: String) -> Stream<Message> {
        return channel.subscribe(key: room)
    }
}
