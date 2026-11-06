/// Why a local-storage read failed.
public enum StorageReadError implements Error, Codable {
    /// Storage is blocked or disabled in this browser.
    case unavailable
    /// Something is stored, but it doesn't decode as the key's type.
    case unreadable
}
