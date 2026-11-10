import k.rpc.RpcFailure

/// Why the server refused one message on a room's channel. The channel
/// stays open.
public enum CommandError implements Error {
    /// The text is blank or longer than 500 characters.
    case invalidText
    /// Only a message's author, or a moderator, deletes it.
    case notYourMessage
    /// The message was already deleted.
    case messageGone
    /// Only a moderator makes moderators and removes members.
    case notModerator
    /// Nobody by that name is a member of the room.
    case noSuchMember
    /// A moderator can't be removed.
    case cannotRemoveModerator
    /// A moderator removed the sender from the room.
    case removedFromRoom
    case rpcFailure(RpcFailure)
}
