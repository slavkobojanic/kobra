// Blocks nest: an `each` inside a `when`, a `when` inside an `each` row; each has its own markers.
struct Todo {
    let id: Int
    let title: String
    let isDone: Bool
}

component Todos {
    @State var todos: List<Todo> = []

    view {
        section {
            if todos.isEmpty {
                paragraph { "Nothing to do" }
            } else {
                list {
                    for todo in todos key todo.id {
                        item {
                            text { todo.title }

                            if todo.isDone {
                                text { "done" }
                            }
                        }
                    }
                }
            }
        }
    }
}
