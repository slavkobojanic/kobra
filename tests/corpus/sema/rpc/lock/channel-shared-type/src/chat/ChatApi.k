import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The chat's implementation.
public service ChatApi implements ChatProtocol {
    rpc last() async throws RpcOnlyError -> ChatEvent {
        return .posted(text: "hi")
    }

    rpc room(id: Int) async throws RpcOnlyError -> Channel<String, ChatEvent, RpcOnlyError> {
        return Channel(sending: events()) { text => }
    }

    private func events() -> Stream<ChatEvent> {
        return events()
    }
}
