import k.device.Notifications
import k.device.SystemNotification
import k.time.Clock

/// Reminders about todos, shown by the operating system half an hour after
/// "Remind me", also while the tab is in the background.
///
/// `Notifications` is a client-only platform interface, so this service is
/// client-only too, which suits an app that renders in the browser anyway.
/// Only `requestAccess()` shows the browser's prompt, and the compiler
/// allows it only from a user's action: `remind(about:)` asks from the
/// row's click, never from the page's effect.
///
/// A reminder lives as long as the tab: it isn't saved, and closing the tab
/// drops it. A click on the notification brings the tab back.
public service ReminderService {
    @Inject let notifications: Notifications
    @Inject let todos: TodoService
    @Inject let clock: Clock

    /// How long after "Remind me" the reminder comes.
    let delay: Duration = 30.minutes

    /// The reminders not shown yet, oldest first.
    @State var pending: List<Reminder> = []

    /// Whether any reminder is waiting, which keeps the page's check running.
    @Derived let hasPending: Bool = pending.isNotEmpty

    /// Hidden where the browser can't notify, and once the user has blocked
    /// notifications for the site. Reactive: it follows the site's settings.
    @Derived let canRemind: Bool = notifications.isSupported and notifications.permission != .denied

    /// Whether a reminder about the todo is waiting.
    func isPending(id: TodoId) -> Bool {
        return pending.any { reminder => reminder.todoId == id }
    }

    /// Sets a reminder about `todo`, asking for permission the first time.
    /// Nothing is set when the user refuses or dismisses the prompt.
    func remind(about todo: Todo) async {
        if notifications.permission == .prompt {
            let answer = await notifications.requestAccess()

            guard answer == .granted else { return }
        }

        guard notifications.permission == .granted else { return }

        pending.append(Reminder(todoId: todo.id, title: todo.title, due: clock.now + delay))
    }

    /// Shows every reminder that is due, unless its todo was done or
    /// deleted since, and forgets it. Each is forgotten after it is shown,
    /// so `hasPending` turns false only once the last one is out.
    func showDue() async {
        let now = clock.now
        let due = pending.filter { reminder => reminder.due <= now }

        for reminder in due {
            let isOpen = todos.items.any { todo => todo.id == reminder.todoId and not todo.isDone }

            if isOpen {
                do {
                    try await notifications.show(SystemNotification(title: reminder.title, body: "You asked to be reminded about this.", tag: "todo-{reminder.todoId.value}", opens: .home))
                } catch _ {
                    // Notifications were turned off since: there is nothing
                    // to show the reminder with.
                }
            }

            pending.removeWhere { other => other == reminder }
        }
    }
}
