// A view `for` without `key` is K3106 at the insertion point after the operand;
// the item type has an `id`, so the fix inserts ` key todo.id`.
struct Todo {
    let id: Int
    let title: String
}

component Todos {
    @Prop let todos: List<Todo>

    view {
        for todo in todos {
            text { todo.title }
        }
    }
}
