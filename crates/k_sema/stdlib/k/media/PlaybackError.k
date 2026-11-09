/// Why playing failed.
public enum PlaybackError implements Error, Equatable {
    /// The browser wants a user's action before playing sound.
    case blocked
    /// The browser can't decode it.
    case unsupportedFormat
    /// It couldn't be loaded.
    case unreachable
}
