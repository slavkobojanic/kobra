// The page is server-rendered; the slot content it passes to the client-rendered component
// renders in the browser, so it may read the client-only service too.
public page Todos {
    @Inject let todos: TodoService

    view {
        heading(level: 1) { "Todos" }
        TodoList(title: "Mine") {
            text { "{todos.items.count} left" }
        }
    }
}
