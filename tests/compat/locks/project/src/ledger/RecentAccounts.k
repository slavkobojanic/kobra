import k.storage.LocalStorage
import k.storage.StorageKey

/// The account the browser showed last, kept in local storage
/// (`storage.lock`).
public service RecentAccounts {
    @Inject let storage: LocalStorage

    private let lastAccount = StorageKey<String>(name: "lastAccount")

    /// The account shown last; `nil` when none was, or it can't be read.
    func last() -> String? {
        do {
            return try storage.read(key: lastAccount)
        } catch .unreadable {
            return nil
        } catch .unavailable {
            return nil
        }
    }
}
