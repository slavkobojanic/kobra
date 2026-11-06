import accounts.AccountId
import k.rpc.Broadcast

/// Fans note changes out to every open `watch()` stream of the same account.
///
/// There is one instance per server process, so a save handled for one tab
/// reaches the account's other tabs and devices. Server-only: only
/// `NotesApi` injects it.
@Scope(.server)
public service NoteChanges {
    private let channel: Broadcast<AccountId, NoteChange> = Broadcast()

    /// Sends a change to every open stream of the note's owner, and to no
    /// other account's.
    ///
    /// Called inside a `database.transaction`, the change is held until the
    /// transaction commits and dropped if it rolls back, so other tabs never
    /// see a change that didn't happen.
    ///
    /// - Parameters:
    ///   - ownerId: the account the change belongs to.
    ///   - change: the change to send.
    func publish(ownerId: AccountId, change: NoteChange) {
        channel.publish(key: ownerId, value: change)
    }

    /// Subscribes to one account's changes.
    ///
    /// The subscription starts immediately and buffers until the stream is
    /// read, so nothing published after this call is missed. It ends when
    /// the stream is closed, for example when the browser tab goes away, and
    /// when the session that opened it ends.
    ///
    /// - Parameter ownerId: the account whose changes to receive.
    func subscribe(ownerId: AccountId) -> Stream<NoteChange> {
        return channel.subscribe(key: ownerId)
    }
}
