import todo.Todo

/// Lists todos, and lets the parent decide how each one is drawn.
///
/// `row` is a scoped slot: `TodoList` calls it with each todo, and the parent
/// returns the view for that todo. The list owns the structure, a semantic
/// list keyed by todo id so that each row keeps its state (such as an edit in
/// progress) while the list around it changes. The parent owns the content.
/// When there are no todos, the `empty` slot is shown instead of the list.
component TodoList {
    /// The todos to show, in display order.
    @Prop let todos: List<Todo>

    /// Shown instead of the list when `todos` is empty. Passed as the
    /// trailing block.
    @Slot let empty: View

    /// Draws one todo. It should return an `item`, so that the list stays a
    /// valid list.
    @Slot let row: (Todo) -> View

    view {
        if todos.isEmpty {
            empty
        } else {
            list {
                for todo in todos key todo.id {
                    row(todo)
                }
            }
            .class(.list)
        }
    }

    style {
        .list {
            margin: 0;
            padding: 0;
            listStyle: none;
        }
    }
}
