import k.auth.GuardRejection

/// Why an admin rpc failed.
public enum AdminError implements Error {
    /// The caller isn't signed in, or isn't an admin.
    case rejected(GuardRejection)
}
