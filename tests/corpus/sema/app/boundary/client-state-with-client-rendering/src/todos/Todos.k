// A page that reads the client-only service's state.
public page Todos {
    @Inject let todos: TodoService

    view {
        text { "{todos.items.count} todos" }
    }
}
