import todo.Filter
import todo.ReminderService
import todo.Todo
import todo.TodoService
import k.time.Clock
import ui.Panel

/// The app's only page: the todo list, its filters and the dark mode switch.
///
/// `Home` holds the one piece of state that belongs to the page rather than
/// to the app, the selected `filter`, and shares it with `FilterBar` through
/// `bind`. The todos come from `TodoService`, so they survive anything that
/// happens to the page. The color scheme belongs to the stdlib's
/// `k.ui.Theme` service, which `ThemeToggle` changes directly.
///
/// While reminders are waiting, the page checks once a minute for those
/// that are due and has the operating system show them. A minute is as often
/// as browsers run a background tab's timers anyway, and the page is the one
/// place that stays mounted whatever the filter.
///
/// It has a named URL because it is the site root, which the URL derived from
/// the page name (`/home`) can't express.
public page Home at "/" {
    @Inject let todos: TodoService
    @Inject let reminders: ReminderService
    @Inject let clock: Clock

    @State var filter: Filter = .all

    @Derived let visibleTodos: List<Todo> = todos.visible(filter: filter)

    @Derived let remainingLabel: String = if todos.remainingCount == 1 {
        "1 item left"
    } else {
        "{todos.remainingCount} items left"
    }

    @Derived let emptyMessage: String = match filter {
        case .all { "Nothing to do yet. Add your first todo above." }
        case .active { "Nothing left to do." }
        case .completed { "Nothing is done yet." }
    }

    @Derived let documentTitle: String = if todos.remainingCount == 0 {
        "Todos"
    } else {
        "Todos ({todos.remainingCount})"
    }

    /// Runs again when `hasPending` changes: it stops once the last reminder
    /// is out, and starts when one is set.
    @Effect
    func showReminders() async {
        guard reminders.hasPending else { return }

        while true {
            await clock.sleep(1.minute)
            await reminders.showDue()
        }
    }

    metadata {
        title(documentTitle)
        description("A small todo list that keeps everything in your browser.")
    }

    view {
        main {
            Panel(title: "Todos") {
                NewTodoField()

                if let problem = todos.problem {
                    StorageNotice(problem: problem)
                }

                TodoList(todos: visibleTodos) {
                    paragraph { emptyMessage }
                        .class(.empty)
                }
                .row { todo =>
                    TodoRow(todo: todo)
                }
            }
            .headingLevel(1)
            .container(.board)
            .toolbar {
                ThemeToggle()
            }
            .actions {
                text { remainingLabel }
                    .class(.remaining)

                FilterBar(selection: bind filter)

                if todos.hasCompleted {
                    button { "Clear completed" }
                        .on(click: todos.clearCompleted)
                }
            }
        }
        .class(.page)
    }

    style {
        .page {
            minHeight: 100vh;
            padding: .lg .md;
            backgroundColor: .background;
            color: .text;
            font: .body;
        }

        .board {
            maxWidth: 40rem;
            margin: 0 auto;
        }

        .empty {
            margin: 0;
            padding: .lg 0;
            color: .muted;
            textAlign: center;
        }

        .remaining {
            marginRight: auto;
            color: .muted;
            fontSize: .sm;
        }
    }
}
