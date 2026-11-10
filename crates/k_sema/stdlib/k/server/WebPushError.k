/// Why [`WebPush.send(_:to:timeToLive:)`] didn't deliver
/// (`docs/design/50-push.md`).
public enum WebPushError implements Error, Equatable {
    /// The subscription is gone (the push service answered `404` or
    /// `410`): delete it.
    case expired
    /// The push service couldn't be reached, timed out, or answered `429`
    /// or a `5xx`: try again later.
    case unavailable
    /// The push service refused the message with `status`, such as `403`
    /// when the subscription was made with another key.
    case rejected(status: Int)
    /// The encrypted message is over the 4,096 bytes push services take.
    case tooLarge
    /// The subscription's keys are malformed, or its endpoint isn't
    /// `https`.
    case invalidSubscription
    /// `K_PUSH_PRIVATE_KEY` isn't set.
    case notConfigured
}
