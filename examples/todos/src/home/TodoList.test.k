import k.crypto.Uuid
import todo.Todo
import todo.TodoId

test TodoList {
    test "the row slot draws each todo, in order" {
        let todos = [sampleTodo(title: "Buy milk"), sampleTodo(title: "Walk the dog")]

        let screen = render(
            TodoList(todos: todos) {
                paragraph { "Nothing here." }
            }
            .row { todo =>
                item { "Row for {todo.title}" }
            }
        )

        expect screen.items().map { row => row.text } == ["Row for Buy milk", "Row for Walk the dog"]
        expect screen.text("Nothing here.").exists == false
    }

    test "an empty list shows the empty slot instead of a list" {
        let screen = render(
            TodoList(todos: []) {
                paragraph { "Nothing here." }
            }
            .row { todo =>
                item { todo.title }
            }
        )

        expect screen.text("Nothing here.").isVisible
        expect screen.list().exists == false
    }
}

/// A todo with a fresh id, for building lists to render.
func sampleTodo(title: String) -> Todo {
    return Todo(id: TodoId(value: Uuid.random()), title: title, isDone: false)
}
