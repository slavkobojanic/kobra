import k.auth.Authenticated
import k.crypto.Uuid

/// The contract between the browser and the server for the signed-in
/// account's notes.
///
/// The `Authenticated` guard sits on the protocol, so it covers every rpc,
/// including ones added later. Each rpc acts only on the caller's own notes.
public protocol NotesProtocol requires Authenticated {
    /// Streams the account's notes for as long as the caller listens: a
    /// `NoteChange.snapshot` first, then every change saved from any tab or
    /// device.
    ///
    /// - Throws: `AccessError.rejected` when nobody is signed in.
    rpc watch() throws AccessError -> Stream<NoteChange>

    /// Returns one note.
    ///
    /// - Parameter id: the note's id, usually from the page URL.
    /// - Throws: `LookupError.notFound` when the account has no note with
    ///   this id, and `.rejected` when nobody is signed in.
    rpc get(id: Uuid) throws LookupError -> NoteSummary

    /// Creates a note. The draft is validated before this runs.
    ///
    /// - Parameter draft: the editor's value.
    /// - Returns: the stored note, at revision 1.
    /// - Throws: `AccessError.rejected` when nobody is signed in.
    rpc create(draft: NoteDraft) throws AccessError -> NoteSummary

    /// Replaces a note's title, text and tags.
    ///
    /// - Parameters:
    ///   - id: the note to change.
    ///   - draft: the edited values, validated before this runs.
    ///   - revision: the `NoteSummary.revision` the edit started from.
    /// - Returns: the stored note, one revision later.
    /// - Throws: `SaveError.conflict` when the note has changed since
    ///   `revision`, `.notFound` when it no longer exists, and `.rejected`
    ///   when nobody is signed in.
    rpc update(id: Uuid, draft: NoteDraft, revision: Int) throws SaveError -> NoteSummary

    /// Pins or unpins a note. This isn't an edit, so the revision and
    /// `updatedAt` stay the same and no open editor gets a conflict.
    ///
    /// - Throws: `LookupError.notFound` when the note no longer exists, and
    ///   `.rejected` when nobody is signed in.
    rpc setPinned(id: Uuid, isPinned: Bool) throws LookupError -> NoteSummary

    /// Deletes a note. Deleting a note that is already gone does nothing, so
    /// two tabs deleting the same note both succeed.
    ///
    /// - Throws: `AccessError.rejected` when nobody is signed in.
    rpc delete(id: Uuid) throws AccessError
}
