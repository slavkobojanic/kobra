import k.rpc.Channel
import k.rpc.ChannelClose

/// A scripted channel for a client service tested without a server
/// (`docs/design/44-channels.md`, "TestChannel"): the test plays the
/// server. Hand [`channel`] to the code under test, then [`deliver`]
/// messages and read what it [`sent`].
///
/// ```k,ignore
/// let room: TestChannel<ChatCommand, ChatEvent, CommandError> = TestChannel()
/// ```
@Native
public class TestChannel<C, S, E> {
    /// Creates an open channel with nothing delivered or sent.
    @Native
    init() {}

    /// The channel the code under test reads and sends on.
    let channel: Channel<C, S, E>
    /// The messages the code under test sent and the channel accepted,
    /// oldest first.
    let sent: List<C>

    /// Plays the server sending `message`: the channel's `messages`
    /// stream yields it. While the channel is dropped it waits for the
    /// resume, or is lost on a restart.
    func deliver(_ message: S)
    /// Makes the next `send` throw `error`, as a server refusal.
    func refuseNext(with error: E)
    /// Drops the connection: the channel goes `reconnecting` and opens
    /// again after the first backoff on the [`TestClock`]. Sends wait
    /// meanwhile, at most 10 seconds. With `resumable: false` it
    /// restarts: what was delivered while dropped is lost, and the test
    /// delivers the new snapshot itself.
    func drop(resumable: Bool = true)
    /// Plays the server closing the channel for `reason`.
    func close(_ reason: ChannelClose)
}
