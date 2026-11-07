/// An entity collection or a query over one, inside a repository
/// (`docs/design/19-persistence.md`, "Queries"). `notes` in `repository
/// NoteRepository for Note` is a `Query<Note>`.
///
/// A query is a description until it is awaited; the compiler translates the
/// lambdas to SQL, and a lambda it can't translate is a compile error.
@ServerOnly
@Native
public class Query<T> {
    /// Narrows the query to the rows for which `predicate` holds (SQL
    /// `WHERE`).
    func filter(_ predicate: (T) -> Bool) -> Query<T>
    /// Orders the rows by `key`, ascending.
    func sortedBy<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// Orders the rows by `key`, descending.
    func sortedByDescending<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// Orders rows that compare equal by the previous order by `key`,
    /// ascending; follows [`sortedBy`] or [`sortedByDescending`].
    func thenBy<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// Orders rows that compare equal by the previous order by `key`,
    /// descending.
    func thenByDescending<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// Keeps at most `count` rows (SQL `LIMIT`).
    func limit(_ count: Int) -> Query<T>
    /// Skips the first `count` rows (SQL `OFFSET`).
    func offset(_ count: Int) -> Query<T>
    /// Loads a relation for every row, with one extra query per relation.
    ///
    /// - Parameter relation: the [`k.db.Relation`] field to load, such as
    ///   `{ n => n.author }`.
    func including<R>(_ relation: (T) -> R) -> Query<T>

    /// Runs the query; awaiting the query itself does the same.
    ///
    /// - Returns: the matching rows, in the query's order.
    func list() async -> List<T>
    /// Runs the query for the first row for which `predicate` holds
    /// (`LIMIT 1`).
    ///
    /// - Returns: the row, or `nil` if none matches.
    func first(_ predicate: (T) -> Bool) async -> T?
    /// Runs the query for exactly one row; panics on zero or several.
    ///
    /// - Returns: the one row for which `predicate` holds.
    func single(_ predicate: (T) -> Bool) async -> T
    /// Counts the matching rows (`SELECT count(*)`).
    func count() async -> Int
    /// Updates the matching rows.
    ///
    /// - Parameter assignments: a lambda made of assignments to the row's
    ///   fields only (`{ n => n.archived = true }`).
    /// - Returns: the number of rows updated.
    func update(_ assignments: (T) -> Void) async -> Int
    /// Deletes the matching rows.
    ///
    /// - Returns: the number of rows deleted.
    func delete() async -> Int

    /// Inserts one row (`docs/design/19-persistence.md`, "Writes").
    func insert(_ entity: T) async
    /// Inserts many rows in one statement.
    func insert(all entities: List<T>) async
    /// Deletes the row with `entity`'s [`k.db.Id`].
    func delete(_ entity: T) async
}
