/// The test's channel connection in `k test`
/// (`docs/design/44-channels.md`, "Testing"): the in-process socket
/// between the test's browser and its server. Channels opened in a test
/// run through the real framing, guards, handlers, acknowledgements and
/// limits, with no socket and no second process; the reconnect delays
/// and the resume window follow the [`TestClock`].
///
/// Going offline is [`TestConnectivity.goOffline()`]'s, which also drops
/// this socket and stops reconnects until `goOnline()`.
@Native
public service TestChannels {
    /// The channels the test's browser has open now, oldest first.
    var open: List<OpenChannel>

    /// Drops the test's socket. With `resumable`, the server keeps every
    /// channel for its resume window (30 seconds of the [`TestClock`]),
    /// the channels go `reconnecting` and resume after their backoff
    /// without losing a message. Without it, the server forgets them, as
    /// after a deploy: they restart, and their handlers run again.
    func drop(resumable: Bool = true)
}
