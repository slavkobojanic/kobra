import k.rpc.Channel

/// The chat's implementation.
public service ChatApi implements ChatProtocol {
    rpc room(id: Int) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        return Channel(sending: events()) { command => }
    }

    private func events() -> Stream<ChatEvent> {
        return events()
    }
}
