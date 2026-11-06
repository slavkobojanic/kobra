/// An entity collection or a query over one, inside a repository
/// (`docs/design/19-persistence.md`, "Queries"). `notes` in `repository
/// NoteRepository for Note` is a `Query<Note>`.
///
/// A query is a description until it is awaited; the compiler translates the
/// lambdas to SQL, and a lambda it can't translate is a compile error.
@ServerOnly
@Native
public class Query<T> {
    /// Rows for which `predicate` holds.
    func filter(_ predicate: (T) -> Bool) -> Query<T>
    /// Rows ordered by `key`, ascending.
    func sortedBy<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// Rows ordered by `key`, descending.
    func sortedByDescending<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// A secondary ascending order.
    func thenBy<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// A secondary descending order.
    func thenByDescending<K>(_ key: (T) -> K) -> Query<T> where K implements Comparable
    /// At most `count` rows.
    func limit(_ count: Int) -> Query<T>
    /// Rows after the first `count`.
    func offset(_ count: Int) -> Query<T>
    /// Loads a relation for every row, with one extra query per relation.
    func including<R>(_ relation: (T) -> R) -> Query<T>

    /// The rows. Awaiting the query runs it.
    func list() async -> List<T>
    /// The first row for which `predicate` holds, `LIMIT 1`.
    func first(_ predicate: (T) -> Bool) async -> T?
    /// Exactly one row; panics on zero or several.
    func single(_ predicate: (T) -> Bool) async -> T
    /// The number of rows.
    func count() async -> Int
    /// Updates the matching rows with assignments only; returns the count.
    func update(_ assignments: (T) -> Void) async -> Int
    /// Deletes the matching rows; returns the count.
    func delete() async -> Int

    /// Inserts one row (`docs/design/19-persistence.md`, "Writes").
    func insert(_ entity: T) async
    /// Inserts many rows in one statement.
    func insert(all entities: List<T>) async
    /// Deletes the row with the entity's id.
    func delete(_ entity: T) async
}
