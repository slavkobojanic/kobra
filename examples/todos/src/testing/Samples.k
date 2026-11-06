import k.crypto.Uuid
import todo.Todo
import todo.TodoId

/// Fixtures for tests in every module.
///
/// `testing` is a test-only module: only `*.test.k` files can see it, and it
/// is never part of `k build` output. Its public extensions are visible to
/// every test file without an import.
public extend Todo {
    /// A todo with a fresh random id, for rendering lists and for seeding
    /// storage directly.
    ///
    /// - Parameters:
    ///   - title: the todo's title, used as given.
    ///   - isDone: whether the todo is done.
    /// - Returns: a new todo. Each call has a different id.
    static func sample(title: String, isDone: Bool = false) -> Todo {
        return Todo(id: TodoId(value: Uuid.random()), title: title, isDone: isDone)
    }
}
