import k.rpc.Channel

public service TalkApi implements TalkProtocol {
    @Inject let rooms: Rooms

    rpc room(id: String) async throws JoinError -> Channel<Command, Event, Refusal> {
        return Channel(sending: rooms.subscribe(room: id)) { command =>
            match command {
                case .say(let text) {
                    guard text.count <= 280 else { throw Refusal.tooLong(limit: 280) }
                    rooms.publish(room: id, event: .said(text: text))
                }
            }
        }
    }
}
