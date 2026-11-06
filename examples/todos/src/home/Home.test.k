import appearance.AppearanceService
import k.test.MemoryStorage
import k.ui.Theme
import todo.TodoService

test Home {
    @Inject let storage: MemoryStorage
    @Inject let todos: TodoService
    @Inject let appearance: AppearanceService
    @Inject let theme: Theme

    test "a new list invites the first todo" {
        let screen = render(Home())

        expect screen.heading(named: "Todos").level == 1
        expect screen.text("Nothing to do yet. Add your first todo above.").isVisible
        expect not screen.list().exists
        expect screen.title == "Todos"
    }

    test "added todos are listed and counted" {
        let screen = render(Home())
        let field = screen.textField(labeled: "New todo")

        field.fill("Buy milk")
        field.press(.enter)
        field.fill("Walk the dog")
        field.press(.enter)

        expect screen.items().count == 2
        expect screen.text("2 items left").isVisible
        expect screen.title == "Todos (2)"
    }

    test "filters narrow the list, and each has its own empty message" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")
        todos.setDone(id: todos.items[0].id, isDone: true)
        let screen = render(Home())

        screen.button(named: "Active").click()

        expect screen.items().count == 1
        expect screen.checkbox(labeled: "Walk the dog").isVisible

        screen.button(named: "Completed").click()

        expect screen.items().count == 1
        expect screen.checkbox(labeled: "Buy milk").isVisible

        screen.checkbox(labeled: "Buy milk").click()

        expect screen.text("Nothing is done yet.").isVisible
    }

    test "Clear completed is offered only while something is done" {
        try todos.add(title: "Buy milk")
        let screen = render(Home())

        expect not screen.button(named: "Clear completed").exists

        screen.checkbox(labeled: "Buy milk").click()
        screen.button(named: "Clear completed").click()

        expect not screen.list().exists
        expect not screen.button(named: "Clear completed").exists
    }

    test "each row acts on its own todo, and the count follows" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")
        try todos.add(title: "Call Ana")
        let screen = render(Home())

        screen.button(named: "Delete Buy milk").click()
        screen.checkbox(labeled: "Call Ana").click()

        expect screen.items().count == 2
        expect screen.checkbox(labeled: "Call Ana").isChecked
        expect screen.text("1 item left").isVisible
        expect screen.title == "Todos (1)"
    }

    test "a storage failure is announced, and the app keeps working" {
        storage.failWrites(error: .quotaExceeded)
        let screen = render(Home())

        screen.textField(labeled: "New todo").fill("Buy milk")
        screen.button(named: "Add").click()

        expect screen.status().text.contains("Storage is full, so your latest change wasn't saved.")
        expect screen.checkbox(labeled: "Buy milk").isVisible

        screen.status().button(named: "Dismiss").click()

        expect not screen.status().exists
    }

    test "the dark mode switch changes the theme variant" {
        let screen = render(Home())

        expect theme.current == .base

        screen.checkbox(labeled: "Dark mode").click()

        expect theme.current == .dark

        screen.checkbox(labeled: "Dark mode").click()

        expect theme.current == .base
    }

    test "a dark scheme chosen earlier is applied on the first render" {
        appearance.choose(scheme: .dark)

        let screen = render(Home())

        expect theme.current == .dark
        expect screen.checkbox(labeled: "Dark mode").isChecked
    }
}
