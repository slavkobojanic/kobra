/// The app's database (`docs/design/19-persistence.md`, "Database and
/// transactions"). Server-only, injectable wherever a repository is.
@ServerOnly
@Native
public service Database {
    /// Runs `block` in one transaction and returns its value. A nested call
    /// is a savepoint.
    ///
    /// - Parameter block: the work to do; repository calls inside it share
    ///   the transaction.
    /// - Returns: what `block` returned, once the transaction committed.
    /// - Throws: whatever `block` throws (the call is error-generic), after
    ///   rolling back.
    func transaction<T, E>(_ block: () async throws E -> T) async throws E -> T
    /// Checks that the database answers, by running `SELECT 1`.
    ///
    /// - Parameter timeout: how long to wait for the answer.
    /// - Returns: `true` if the database answered in time; never throws.
    func ping(timeout: Duration) async -> Bool
}
