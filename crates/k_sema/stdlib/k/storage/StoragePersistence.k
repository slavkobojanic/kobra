/// Whether the browser may evict this origin's storage
/// ([`Collections.requestPersistence()`], [`k.crypto.KeyStore.requestPersistence()`]).
public enum StoragePersistence implements Equatable {
    /// The browser won't evict the origin's storage.
    case persistent
    /// It may, under storage pressure.
    case bestEffort
}
