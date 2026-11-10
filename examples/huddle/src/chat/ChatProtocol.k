import k.auth.Authenticated
import k.auth.RateLimit
import k.rpc.Channel

/// The chat, for signed-in people only.
///
/// `room(name:)` is a **channel**: one call opens a long-lived, two-way
/// conversation with the server, carried with every other channel of the
/// tab on one WebSocket. The browser sends `ChatCommand`s, the server sends
/// `ChatEvent`s, and the server may refuse one command with a
/// `CommandError` without closing the channel.
public protocol ChatProtocol requires Authenticated {
    /// Joins a room until the caller leaves it: a snapshot first, then
    /// every event.
    ///
    /// - Throws: `RoomError.passwordRequired` for a room with a password
    ///   the caller hasn't given (see `unlock`), `.removed` for someone a
    ///   moderator removed.
    rpc room(name: String) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>

    /// Gives a room's password, once: the server remembers that the caller
    /// may join. Five tries a minute, so a password can't be guessed.
    rpc unlock(room: String, password: String) throws RoomError requires RateLimit(5.perMinute)
}
