import todo.TitleError
import todo.TodoService

/// A text field and button that add a todo.
///
/// Pressing Enter in the field submits the form, just as the Add button does.
/// After a successful add the field is cleared. After a rejected one it keeps
/// its text, so the user can fix the title instead of typing it again.
///
/// The rules for titles live in `TodoService`, not here: the component only
/// shows the `TitleError` the service throws.
component NewTodoField {
    @Inject let todos: TodoService

    @State var title: String = ""
    @State var error: TitleError? = nil

    func add() {
        do {
            try todos.add(title: title)
            title = ""
            error = nil
        } catch let rejection {
            error = rejection
        }
    }

    view {
        form {
            textField(value: bind title)
                .label("New todo")
                .placeholder("What needs doing?")
                .class(.field)

            button { "Add" }
                .type(.submit)
                .class(.add)

            if let error {
                paragraph { error.message }
                    .role(.alert)
                    .class(.error)
            }
        }
        .class(.form)
        .on(submit: add)
    }

    style {
        .form {
            display: flex;
            flexWrap: wrap;
            gap: .sm;
        }

        .field {
            flex: 1;
            minWidth: 12rem;
        }

        .add {
            padding: .sm .md;
            border: none;
            borderRadius: .sm;
            backgroundColor: .primary;
            color: .onPrimary;

            &:hover { backgroundColor: .primary.opacity(0.85); }
        }

        .error {
            flexBasis: 100%;
            margin: 0;
            color: .danger;
            fontSize: .sm;
        }
    }
}
