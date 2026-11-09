import k.test.MemoryStorage
import k.test.TestClock
import k.test.TestNotifications
import k.ui.Theme
import todo.TodoService

test Home {
    @Inject let storage: MemoryStorage
    @Inject let todos: TodoService
    @Inject let theme: Theme
    @Inject let notifications: TestNotifications
    @Inject let clock: TestClock

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

    test "the switch shows a dark variant applied before the first render" {
        theme.apply(.dark)

        let screen = render(Home())

        expect screen.checkbox(labeled: "Dark mode").isChecked
    }

    test "two todos can be renamed at once, each in its own field" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")
        let screen = render(Home())

        screen.button(named: "Edit Buy milk").click()
        screen.button(named: "Edit Walk the dog").click()
        screen.textField(labeled: "New title for Walk the dog").fill("Walk the cat")
        screen.button(named: "Save Walk the dog").click()

        expect screen.checkbox(labeled: "Walk the cat").isVisible
        expect screen.textField(labeled: "New title for Buy milk").value == "Buy milk"
    }

    test "a reminder comes half an hour later, and its click brings the list back" {
        let screen = render(Home())
        let field = screen.textField(labeled: "New todo")
        field.fill("Buy milk")
        field.press(.enter)

        screen.button(named: "Remind me about Buy milk").click()
        await screen.settle()
        clock.advance(by: 29.minutes)
        await screen.settle()

        expect notifications.shown.isEmpty

        clock.advance(by: 1.minute)
        await screen.settle()

        expect notifications.shown.count == 1
        expect notifications.shown[0].title == "Buy milk"
        expect not screen.text("Reminder set").exists
        expect screen.button(named: "Remind me about Buy milk").isVisible

        notifications.click(notifications.shown[0])
        await screen.settle()

        expect screen.currentPage == .home
    }
}
