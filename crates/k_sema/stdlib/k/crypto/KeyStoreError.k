/// Why a [`KeyStore`] call failed.
public enum KeyStoreError implements Error, Equatable {
    /// The browser's IndexedDB is blocked, as in some private modes.
    case unavailable
}
