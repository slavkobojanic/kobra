/// Where a standard-library declaration can be used.
public enum PlatformSide {
    /// Only on the server: server-only code may use it.
    case server
    /// Only in the browser: client-only code may use it.
    case client
    /// On both sides.
    case both
}
