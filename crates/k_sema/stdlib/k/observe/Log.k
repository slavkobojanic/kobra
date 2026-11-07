/// Structured logging to stdout as JSON lines. Logging a field named
/// `password`, or a [`k.core.Secret`], is a warning.
@Native
public class Log {
    /// Writes `message` at debug level.
    static func debug(_ message: String)
    /// Writes `message` at info level.
    static func info(_ message: String)
    /// Writes `message` at warning level.
    static func warning(_ message: String)
    /// Writes `message` at error level.
    static func error(_ message: String)
}
