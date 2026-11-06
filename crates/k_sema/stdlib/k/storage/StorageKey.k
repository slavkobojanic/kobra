/// A name paired with the type stored under it:
/// `StorageKey<List<Todo>> = StorageKey(name: "todos")`. The name is a
/// string literal, declared once per app and recorded in `storage.lock`.
@Native
public struct StorageKey<T> where T implements Codable {
    /// A key named `name`, a string literal.
    @Native
    init(name: String) {}

    /// The key's name.
    var name: String
}
