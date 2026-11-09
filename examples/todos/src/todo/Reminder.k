/// A reminder about one todo, due at a time. Reminders are kept only while
/// the tab is open: they are not saved with the todos.
public struct Reminder implements Equatable {
    /// The todo it is about.
    let todoId: TodoId

    /// The todo's title when the reminder was set, for the notification.
    let title: String

    /// When to remind.
    let due: Date
}
