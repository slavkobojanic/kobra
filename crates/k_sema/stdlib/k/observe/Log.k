/// Structured logging to stdout as JSON lines. Logging a field named
/// `password`, or a `Secret`, is a warning.
@Native
public class Log {
    /// Logs at debug level.
    static func debug(_ message: String)
    /// Logs at info level.
    static func info(_ message: String)
    /// Logs at warning level.
    static func warning(_ message: String)
    /// Logs at error level.
    static func error(_ message: String)
}
