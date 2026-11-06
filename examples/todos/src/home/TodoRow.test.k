import k.storage.LocalStorage
import k.test.MemoryStorage
import todo.TodoService

test TodoRow {
    use MemoryStorage for LocalStorage

    @Inject let todos: TodoService

    test "checking the box marks the todo done" {
        try todos.add(title: "Buy milk")
        let screen = render(TodoRow(todo: todos.items[0]))

        screen.checkbox(labeled: "Buy milk").click()

        expect todos.items[0].isDone
    }

    test "a done todo is checked, and unchecking it marks it not done" {
        try todos.add(title: "Buy milk")
        todos.setDone(id: todos.items[0].id, isDone: true)
        let screen = render(TodoRow(todo: todos.items[0]))

        expect screen.checkbox(labeled: "Buy milk").isChecked

        screen.checkbox(labeled: "Buy milk").click()

        expect todos.items[0].isDone == false
    }

    test "an edit starts from the current title, and saving it renames the todo" {
        try todos.add(title: "Buy milk")
        let screen = render(TodoRow(todo: todos.items[0]))

        screen.button(named: "Edit").click()

        expect screen.textField(labeled: "Title").value == "Buy milk"

        screen.textField(labeled: "Title").fill("Buy oat milk")
        screen.button(named: "Save").click()

        expect todos.items[0].title == "Buy oat milk"
        expect screen.textField(labeled: "Title").exists == false
    }

    test "cancelling an edit keeps the old title" {
        try todos.add(title: "Buy milk")
        let screen = render(TodoRow(todo: todos.items[0]))

        screen.button(named: "Edit").click()
        screen.textField(labeled: "Title").fill("Buy oat milk")
        screen.button(named: "Cancel").click()

        expect todos.items[0].title == "Buy milk"
        expect screen.checkbox(labeled: "Buy milk").isVisible
    }

    test "Save is disabled while the draft is blank" {
        try todos.add(title: "Buy milk")
        let screen = render(TodoRow(todo: todos.items[0]))

        screen.button(named: "Edit").click()
        screen.textField(labeled: "Title").fill("   ")

        expect screen.button(named: "Save").isDisabled
    }

    test "a title that is too long is explained and editing continues" {
        try todos.add(title: "Buy milk")
        let screen = render(TodoRow(todo: todos.items[0]))

        screen.button(named: "Edit").click()
        screen.textField(labeled: "Title").fill("a".repeated(times: todos.maximumTitleLength + 1))
        screen.button(named: "Save").click()

        expect screen.alert().text == "Keep the title to 200 characters or fewer."
        expect screen.textField(labeled: "Title").exists
        expect todos.items[0].title == "Buy milk"
    }

    test "deleting removes the todo" {
        try todos.add(title: "Buy milk")
        let screen = render(TodoRow(todo: todos.items[0]))

        screen.button(named: "Delete").click()

        expect todos.items.isEmpty
    }
}
