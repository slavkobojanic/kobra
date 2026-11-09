import k.rpc.Channel

public protocol TalkProtocol {
    rpc room(id: String) throws JoinError -> Channel<Command, Event, Refusal>
}
