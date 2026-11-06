/// The app's database (`docs/design/19-persistence.md`, "Database and
/// transactions"). Server-only, injectable wherever a repository is.
@ServerOnly
@Native
public service Database {
    /// Runs `block` in one transaction and returns its value. Error-generic:
    /// the call throws what the block throws, after rolling back; a nested
    /// call is a savepoint.
    func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T
    /// Runs `SELECT 1`; never throws.
    func ping(timeout: Duration) async -> Bool
}
