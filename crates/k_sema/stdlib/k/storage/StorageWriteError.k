/// Why a local-storage write failed.
public enum StorageWriteError implements Error, Codable {
    /// Storage is blocked or disabled in this browser.
    case unavailable
    /// The origin's storage quota is used up.
    case quotaExceeded
}
