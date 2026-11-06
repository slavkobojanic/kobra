/// One thing to do.
///
/// `Todo` is a value: assigning it copies it, so holding a `Todo` never lets
/// code change the list behind `TodoService`'s back. The service changes its
/// todos in place (`items[index].isDone = true`), and because nested `@State`
/// updates are fine-grained, only the DOM nodes that read that one field
/// update.
public struct Todo implements Codable, Equatable {
    /// Identifies the todo. Never changes.
    let id: TodoId

    /// What needs doing. Always trimmed, never empty, and at most
    /// `TodoService.maximumTitleLength` characters long.
    var title: String

    /// Whether the todo has been done.
    var isDone: Bool
}
