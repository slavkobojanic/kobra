import k.rpc.Channel

/// The page's side of the room.
public service Talker {
    @Inject let api: TalkProtocol

    @State var last: String = ""
    @State var channel: Channel<Command, Event, Refusal>? = nil

    func follow() async throws JoinError {
        let opened = try await api.room(id: "lobby")
        channel = opened
        for await event in opened.messages {
            match event {
                case .said(let text) { last = text }
            }
        }
    }

    func say(text: String) async throws Refusal {
        try await channel?.send(.say(text: text))
    }
}
