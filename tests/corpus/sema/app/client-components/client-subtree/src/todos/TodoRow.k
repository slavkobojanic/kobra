// Rendered only inside `TodoList`'s view, so it renders in the browser only: reading the
// client-only service is fine.
public component TodoRow {
    @Prop let text: String

    @Inject let todos: TodoService

    view {
        paragraph { "{text} of {todos.items.count}" }
    }
}
