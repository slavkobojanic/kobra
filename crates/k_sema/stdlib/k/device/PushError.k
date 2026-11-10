/// Why [`PushSubscriptions.subscribe(serverKey:)`] didn't subscribe
/// (`docs/design/50-push.md`).
public enum PushError implements Error, Equatable {
    /// This browser or context can't receive push messages.
    case unsupported
    /// The user hasn't been asked yet: call
    /// [`PushSubscriptions.requestAccess()`] from a user's action first.
    case notAsked
    /// The user, a browser setting or a policy refused notifications.
    case denied
    /// The browser couldn't reach its push service, or it refused.
    case unavailable
}
