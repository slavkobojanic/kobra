import todo.Todo

test TodoList {
    test "the row slot draws each todo, in order" {
        let todos: List<Todo> = [.sample(title: "Buy milk"), .sample(title: "Walk the dog")]

        let screen = render(
            TodoList(todos: todos) {
                paragraph { "Nothing here." }
            }
            .row { todo =>
                item { "Row for {todo.title}" }
            }
        )

        expect screen.items().map { row => row.text } == ["Row for Buy milk", "Row for Walk the dog"]
        expect not screen.text("Nothing here.").exists
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
        expect not screen.list().exists
    }
}
