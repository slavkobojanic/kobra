import k.ui.VisuallyHidden
import todo.ReminderService
import todo.TitleError
import todo.Todo
import todo.TodoService

/// One todo in the list: a checkbox to mark it done, and buttons to rename or
/// delete it.
///
/// Every row has its own Edit and Delete buttons, so each button's accessible
/// name ends with the todo's title ("Delete Buy milk"), added for screen
/// readers with `VisuallyHidden`. The visible text still starts the name, so
/// voice-control users can say "Delete" as they see it.
///
/// Renaming happens in place. While editing, the row shows a small form whose
/// draft lives in this component until it is saved, so cancelling leaves the
/// todo untouched. Several rows can be edited at once, so the form's field
/// and buttons are named after the todo too ("New title for Buy milk",
/// "Save Buy milk"). The label uses the saved title, which doesn't change
/// while the draft is typed. Opening the form moves the focus into its
/// field (`.autofocus`), so a keyboard user can type at once.
///
/// Where the browser can show notifications, an open todo also has "Remind
/// me", which asks for permission from the click the first time and sets a
/// reminder half an hour away (`ReminderService`).
component TodoRow {
    /// The todo to show. Changes go through `TodoService`, never through this
    /// value.
    @Prop let todo: Todo

    @Inject let todos: TodoService
    @Inject let reminders: ReminderService

    @State var isEditing: Bool = false
    @State var draft: String = ""
    @State var error: TitleError? = nil

    @Derived let isDraftBlank: Bool = draft.trimmed().isEmpty

    func markDone(isDone: Bool) {
        todos.setDone(id: todo.id, isDone: isDone)
    }

    func startEditing() {
        draft = todo.title
        error = nil
        isEditing = true
    }

    func cancelEditing() {
        isEditing = false
    }

    func saveTitle() {
        do {
            try todos.rename(id: todo.id, title: draft)
            isEditing = false
        } catch let rejection {
            error = rejection
        }
    }

    func delete() {
        todos.remove(id: todo.id)
    }

    /// A click is a user's action, so the reminder may ask for permission
    /// from here.
    func remind() async {
        await reminders.remind(about: todo)
    }

    view {
        item {
            if isEditing {
                form {
                    textField(value: bind draft)
                        .label("New title for {todo.title}")
                        .autofocus(true)
                        .class(.field)

                    button {
                        "Save"

                        VisuallyHidden { " {todo.title}" }
                    }
                    .type(.submit)
                    .disabled(isDraftBlank)

                    button {
                        "Cancel"

                        VisuallyHidden { " {todo.title}" }
                    }
                    .type(.button)
                    .on(click: cancelEditing)

                    if let error {
                        paragraph { error.message }
                            .role(.alert)
                            .class(.error)
                    }
                }
                .class(.editor)
                .on(submit: saveTitle)
            } else {
                checkbox(isChecked: Binding(get: { todo.isDone }, set: markDone))
                    .label(todo.title)

                button {
                    "Edit"

                    VisuallyHidden { " {todo.title}" }
                }
                .class(.edit)
                .on(click: startEditing)

                if reminders.canRemind and not todo.isDone {
                    if reminders.isPending(id: todo.id) {
                        text { "Reminder set" }
                            .class(.reminder)
                    } else {
                        button {
                            "Remind me"

                            VisuallyHidden { " about {todo.title}" }
                        }
                        .on(click: remind)
                    }
                }

                button {
                    "Delete"

                    VisuallyHidden { " {todo.title}" }
                }
                .class(.delete)
                .on(click: delete)
            }
        }
        .class([.row, .done when todo.isDone, .editing when isEditing])
    }

    style {
        .row {
            display: flex;
            alignItems: center;
            gap: .sm;
            padding: .sm;
            borderBottom: 1px solid .border;
        }

        .done { color: .muted; textDecoration: lineThrough; }
        .editing { backgroundColor: .background; }
        .edit { marginLeft: auto; }
        .delete { color: .danger; }
        .reminder { color: .muted; fontSize: .sm; }

        .editor {
            display: flex;
            flexWrap: wrap;
            gap: .sm;
            width: 100%;
        }

        .field { flex: 1; }

        .error {
            flexBasis: 100%;
            margin: 0;
            color: .danger;
            fontSize: .sm;
        }
    }
}
