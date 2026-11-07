/// Where a standard-library declaration can be used, the argument of
/// [`Platform`].
public enum PlatformSide {
    /// Only on the server: server-only code may use it.
    case server
    /// Only in the browser: client-only code may use it.
    case client
    /// On both sides.
    case both
}
