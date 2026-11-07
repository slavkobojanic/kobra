// `key item.id` keys rows by a member; the row reads its item through a getter, so an update with
// the same key re-renders in place (runtime.md §5.3).
struct Todo {
    let id: Int
    let title: String
}

component Todos {
    @State var todos: List<Todo> = []

    view {
        list {
            for todo in todos key todo.id {
                item { todo.title }
            }
        }
    }
}
