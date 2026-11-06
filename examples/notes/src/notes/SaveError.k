import k.auth.GuardRejection

/// Why saving an edit to a note failed.
public enum SaveError implements Error {
    /// There is no note with this id, or it belongs to another account. It
    /// may have been deleted in another tab.
    case notFound

    /// The note was saved elsewhere, for example in another tab, after this
    /// edit started. `latest` is that newer version, so the person can decide
    /// whose changes to keep.
    case conflict(latest: NoteSummary)

    /// The `Authenticated` guard refused the call: nobody is signed in.
    case rejected(GuardRejection)
}
