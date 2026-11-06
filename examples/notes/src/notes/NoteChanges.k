import k.rpc.Broadcast

/// Fans note changes out to every open `watch()` stream of the same account.
///
/// There is one instance per server process, so a save handled for one tab
/// reaches the account's other tabs and devices. Server-only: only
/// `NotesApi` injects it.
@Scope(.server)
public service NoteChanges {
    private let channel: Broadcast<OwnedChange> = Broadcast()

    /// Sends a change to every open stream of the note's owner. Call it only
    /// after the change has been committed.
    ///
    /// - Parameters:
    ///   - ownerId: the session user id of the account the change belongs to.
    ///   - change: the change to send.
    public func publish(ownerId: String, change: NoteChange) {
        channel.publish(OwnedChange(ownerId: ownerId, change: change))
    }

    /// Subscribes to one account's changes.
    ///
    /// The subscription starts immediately and buffers until the stream is
    /// read, so nothing published after this call is missed. It ends when
    /// the stream is closed, for example when the browser tab goes away.
    ///
    /// - Parameter ownerId: the session user id of the account.
    public func subscribe(ownerId: String) -> Stream<NoteChange> {
        return channel
            .subscribe()
            .filter { owned => owned.ownerId == ownerId }
            .map { owned => owned.change }
    }
}

/// A change together with the account it belongs to. Never leaves the server.
struct OwnedChange {
    let ownerId: String
    let change: NoteChange
}
