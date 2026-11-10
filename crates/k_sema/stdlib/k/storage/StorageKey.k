/// A name paired with the type stored under it:
/// `let savedTodos = StorageKey<List<Todo>>(name: "todos")`. The name is a
/// string literal, declared once per app and recorded in `storage.lock`.
@Native
public struct StorageKey<T> where T implements Codable {
    /// Creates the key named `name`.
    ///
    /// - Parameter name: a string literal, unique in the app.
    @Native
    @Literal
    init(name: String) {}

    /// The key's name.
    var name: String
}
