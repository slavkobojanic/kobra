import k.auth.Authenticated
import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.rpc.RpcOnlyError

/// "Notify me" for a room, for signed-in people only
/// (`docs/design/50-push.md`).
public protocol AlertsProtocol requires Authenticated {
    /// The key browsers subscribe with, or `nil` when the server has no
    /// `K_PUSH_PRIVATE_KEY` and pushes nothing.
    rpc serverKey() throws RpcOnlyError -> EcdsaP256PublicKey?

    /// Pushes new messages of `room` to `subscription`, for the caller.
    rpc follow(room: String, subscription: PushSubscription) throws RoomError

    /// Stops pushing `room`'s messages to `subscription`.
    rpc unfollow(room: String, subscription: PushSubscription) throws RpcOnlyError

    /// Whether `room`'s messages are pushed to `subscription`.
    rpc isFollowing(room: String, subscription: PushSubscription) throws RpcOnlyError -> Bool
}
