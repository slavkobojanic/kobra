import todo.TitleError
import todo.Todo
import todo.TodoService

/// One todo in the list: a checkbox to mark it done, and buttons to rename or
/// delete it.
///
/// Renaming happens in place. While editing, the row shows a small form whose
/// draft lives in this component until it is saved, so cancelling leaves the
/// todo untouched.
component TodoRow {
    /// The todo to show. Changes go through `TodoService`, never through this
    /// value.
    @Prop let todo: Todo

    @Inject let todos: TodoService

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

    view {
        item {
            if isEditing {
                form {
                    textField(value: bind draft)
                        .label("Title")
                        .class(.field)

                    button { "Save" }
                        .type(.submit)
                        .disabled(isDraftBlank)

                    button { "Cancel" }
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

                button { "Edit" }
                    .class(.edit)
                    .on(click: startEditing)

                button { "Delete" }
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

        .done    { color: .muted; textDecoration: line-through; }
        .editing { backgroundColor: .background; }
        .edit    { marginLeft: auto; }
        .delete  { color: .danger; }

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
