/// The delays between reconnection attempts that channels use
/// (`docs/design/44-channels.md`, "Reconnecting and resuming"), for a loop
/// that reconnects by hand, such as one over a [`k.http.WebSocket`]:
/// a random delay up to 0.5 s, 1 s, 2 s, 4 s, 8 s, 16 s, then up to 30 s
/// for every later attempt ("full jitter").
///
/// ```k,ignore
/// let backoff = Backoff()
/// while true {
///     do {
///         let socket = try await sockets.connect(request: request)
///         backoff.reset()
///         for await message in socket.messages { … }
///     } catch _ {}
///     await clock.sleep(backoff.next())
/// }
/// ```
///
/// The jitter draws from the runtime's random source, which `k test` seeds
/// per test, so a test's delays repeat; sleeping on the injected [`Clock`]
/// lets [`k.test.TestClock`] drive them.
@Native
public class Backoff {
    /// A backoff before its first attempt.
    @Native
    init() {}

    /// The delay before the next attempt, which counts it.
    func next() -> Duration
    /// Starts again from the first attempt's delay, after a success.
    func reset()
}
