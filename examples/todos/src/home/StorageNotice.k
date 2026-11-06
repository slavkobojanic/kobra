import todo.StorageProblem
import todo.TodoService

/// Tells the user that their todos could not be loaded, or are not being
/// saved.
///
/// The notice has the `status` role, so screen readers announce it when it
/// appears without interrupting what the user is doing.
component StorageNotice {
    /// The problem to explain.
    @Prop let problem: StorageProblem

    @Inject let todos: TodoService

    view {
        box {
            match problem {
                case .unreadable {
                    paragraph { "Your saved todos couldn't be read, so the list started empty." }
                        .class(.message)
                }

                case .unavailable {
                    paragraph { "This browser isn't allowing storage. Your todos will be gone when you close the tab." }
                        .class(.message)
                }

                case .full {
                    paragraph { "Storage is full, so your latest change wasn't saved." }
                        .class(.message)
                }
            }

            button { "Dismiss" }
                .on(click: todos.dismissProblem)
        }
        .role(.status)
        .class(.notice)
    }

    style {
        .notice {
            display: flex;
            alignItems: center;
            justifyContent: space-between;
            gap: .md;
            padding: .sm .md;
            borderRadius: .sm;
            backgroundColor: .warningSurface;
            color: .warningText;
        }

        .message { margin: 0; }
    }
}
