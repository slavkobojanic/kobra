/// A long-lived, two-way, typed conversation between one browser tab and
/// the app's server (`docs/design/44-channels.md`): the result of a
/// protocol's channel rpc,
/// `rpc room(id: RoomId) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>`.
/// The client sends `C`, the server sends `S`, and the server may refuse a
/// client message with the error enum `E`, which has an
/// `rpcFailure(RpcFailure)` case like every rpc error.
///
/// On the server a channel is only built, by the handler of a channel
/// rpc, and returned; its `state`, `messages`, `send` and `close` are the
/// client's.
@Native
public class Channel<C, S, E> {
    /// Builds the channel a handler returns. Server-only: only the handler
    /// of a channel rpc builds one.
    ///
    /// - Parameters:
    ///   - sending: every message the server sends on the channel; when it
    ///     ends, the server has closed the channel
    ///     ([`ChannelClose.byServer`]).
    ///   - closed: runs on a normal close, best-effort.
    ///   - receiving: runs for each client message, in order, one at a
    ///     time; throwing `E` refuses that message.
    @Native
    init(sending: Stream<S>, closed: ((ChannelClose) -> Void)? = nil, receiving: (C) async throws E -> Void) {}

    /// `connecting`, `open`, `reconnecting` or `closed`; reactive.
    /// Client-only.
    let state: ChannelState
    /// Every server message, in order, across reconnects; ends when the
    /// channel closes for good. Read once. Client-only.
    let messages: Stream<S>

    /// Sends `message` and waits for the server's acknowledgement.
    /// Client-only.
    ///
    /// - Throws: the server's refusal, or `rpcFailure`; on a closed channel,
    ///   `rpcFailure(.offline)` at once.
    func send(_ message: C) async throws E
    /// Closes the channel for good; idempotent. Client-only.
    func close()
}
