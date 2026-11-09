import k.device.SystemNotification
import k.test.TestClock
import k.test.TestNotifications

test ReminderService {
    @Inject let reminders: ReminderService
    @Inject let todos: TodoService
    @Inject let notifications: TestNotifications
    @Inject let clock: TestClock

    test "a reminder is shown half an hour later, and opens the list" {
        try todos.add(title: "Buy milk")
        await reminders.remind(about: todos.items[0])

        expect reminders.isPending(id: todos.items[0].id)

        clock.advance(by: 29.minutes)
        await reminders.showDue()

        expect notifications.shown.isEmpty

        clock.advance(by: 1.minute)
        await reminders.showDue()

        expect notifications.shown == [SystemNotification(title: "Buy milk", body: "You asked to be reminded about this.", tag: "todo-{todos.items[0].id.value}", opens: .home)]
        expect not reminders.hasPending
    }

    test "asking happens once; a refused or dismissed prompt sets nothing" {
        try todos.add(title: "Buy milk")
        notifications.answerPrompts(with: .prompt)
        await reminders.remind(about: todos.items[0])

        expect not reminders.hasPending

        notifications.answerPrompts(with: .granted)
        await reminders.remind(about: todos.items[0])
        await reminders.remind(about: todos.items[0])

        expect notifications.prompts == 2
        expect reminders.pending.count == 2
    }

    test "a todo done or deleted before its time isn't reminded about" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Call Ada")
        notifications.setPermission(.granted)
        await reminders.remind(about: todos.items[0])
        await reminders.remind(about: todos.items[1])
        todos.setDone(id: todos.items[0].id, isDone: true)
        todos.remove(id: todos.items[1].id)

        clock.advance(by: 30.minutes)
        await reminders.showDue()

        expect notifications.shown.isEmpty
        expect not reminders.hasPending
    }

    test "notifications turned off since the reminder was set: it is dropped quietly" {
        try todos.add(title: "Buy milk")
        notifications.setPermission(.granted)
        await reminders.remind(about: todos.items[0])
        notifications.setPermission(.denied)

        clock.advance(by: 30.minutes)
        await reminders.showDue()

        expect notifications.shown.isEmpty
        expect not reminders.hasPending
        expect not reminders.canRemind
    }
}
