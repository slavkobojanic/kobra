/// The guard that limits how often a client may call: `requires
/// RateLimit(10.perMinute)`. Counted per session for signed-in clients and
/// per client address for guests.
@Guard
public struct RateLimit {
    /// Allows at most `rate`.
    @Native
    init(_ rate: Rate) {}
}
