import k.crypto.Uuid

/// A browser that asked to be told about new messages in a room, stored in
/// the `room_alert` table (`docs/design/50-push.md`).
public entity RoomAlert {
    @Id let id: Uuid

    /// The room it follows.
    @Index let room: String

    /// Who asked. Their own messages are never pushed to them.
    let member: String

    /// The browser's `PushSubscription`, in three columns: the push
    /// service's URL for the browser, which identifies it (one alert per
    /// room and browser), and the keys messages are encrypted to.
    @Index let endpoint: Url
    let p256dh: String
    let auth: String
}
