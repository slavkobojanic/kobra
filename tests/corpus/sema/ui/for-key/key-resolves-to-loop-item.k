// `for item in items key item.id`: the key reads the loop variable; the loop
// variable has the element type.
struct Todo {
    let id: Int
    let title: String
}

component Todos {
    @Prop let todos: List<Todo>

    view {
        for todo in todos key todo.id {
          //^^^^ type: corpus.Todo
            //                ^^^^^^^ type: Int
            text { todo.title }
        }
    }
}
