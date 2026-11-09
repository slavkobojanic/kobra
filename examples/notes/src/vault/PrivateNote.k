import k.auth.Authenticated
import k.crypto.Uuid

/// A note's private text, sealed in this browser with the account's vault
/// (`docs/design/33-crypto.md`, "Example: private notes"). The server
/// stores and syncs it but can't read it; the title and tags stay plain, so
/// the list and search still work.
public page PrivateNote requires Authenticated {
    /// The note whose private text this is.
    @Param let id: Uuid

    metadata {
        title("Private text · K Notes")
    }

    view {
        heading(level: 1) { "Private text" }

        PrivateTextEditor(note: id)

        link(to: .editNote(id: id)) { "Back to the note" }
    }
}
