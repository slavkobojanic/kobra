import todo.TodoService

test NewTodoField {
    @Inject let todos: TodoService

    test "clicking Add adds the todo and clears the field" {
        let screen = render(NewTodoField())

        screen.textField(labeled: "New todo").fill("Buy milk")
        screen.button(named: "Add").click()

        expect todos.items.map { todo => todo.title } == ["Buy milk"]
        expect screen.textField(labeled: "New todo").value == ""
    }

    test "the field has the focus again after clicking Add" {
        let screen = render(NewTodoField())

        screen.textField(labeled: "New todo").fill("Buy milk")
        screen.button(named: "Add").click()

        expect screen.textField(labeled: "New todo").isFocused
    }

    test "a rejected title leaves the focus in the field to fix it" {
        let screen = render(NewTodoField())

        screen.button(named: "Add").click()

        expect screen.alert().exists
        expect screen.textField(labeled: "New todo").isFocused
    }

    test "pressing Enter in the field adds the todo" {
        let screen = render(NewTodoField())

        screen.textField(labeled: "New todo").fill("Buy milk")
        screen.textField(labeled: "New todo").press(.enter)

        expect todos.items.count == 1
    }

    test "a blank title is explained and nothing is added" {
        let screen = render(NewTodoField())

        screen.textField(labeled: "New todo").fill("   ")
        screen.button(named: "Add").click()

        expect screen.alert().text == "Give the todo a title."
        expect todos.items.isEmpty
    }

    test "a title that is too long stays in the field so it can be fixed" {
        let tooLong = "a".repeated(times: todos.maximumTitleLength + 1)
        let screen = render(NewTodoField())

        screen.textField(labeled: "New todo").fill(tooLong)
        screen.button(named: "Add").click()

        expect screen.alert().text == "Keep the title to 200 characters or fewer."
        expect screen.textField(labeled: "New todo").value == tooLong
        expect todos.items.isEmpty
    }

    test "the error goes away after a successful add" {
        let screen = render(NewTodoField())

        screen.button(named: "Add").click()
        screen.textField(labeled: "New todo").fill("Buy milk")
        screen.button(named: "Add").click()

        expect not screen.alert().exists
    }
}
