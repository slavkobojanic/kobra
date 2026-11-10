import k.device.PushSubscription
import k.device.SystemNotification

/// One message [`RecordingWebPush`] recorded.
public struct SentPush implements Equatable {
    /// What was sent.
    let message: SystemNotification
    /// Where to.
    let subscription: PushSubscription
    /// The time to live it was sent with.
    let timeToLive: Duration
}
