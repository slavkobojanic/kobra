// A key expression is type-checked: an unknown member is K1203, and no K3106 or
// K2009 follows from it.
struct Todo {
    let title: String
}

component Todos {
    @Prop let todos: List<Todo>

    view {
        for todo in todos key todo.id {
            text { todo.title }
        }
    }
}
