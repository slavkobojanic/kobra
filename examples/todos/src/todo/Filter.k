/// Which todos the list shows.
public enum Filter implements Equatable, Hashable {
    /// Every todo.
    case all

    /// Todos that are not done yet.
    case active

    /// Todos that are done.
    case completed

    /// Whether `todo` belongs in the list while this filter is selected.
    ///
    /// - Parameter todo: the todo to test.
    /// - Returns: `true` for every todo under `.all`; otherwise `true` when
    ///   the todo's `isDone` matches the filter.
    func includes(todo: Todo) -> Bool {
        return match self {
            case .all { true }
            case .active { not todo.isDone }
            case .completed { todo.isDone }
        }
    }
}
