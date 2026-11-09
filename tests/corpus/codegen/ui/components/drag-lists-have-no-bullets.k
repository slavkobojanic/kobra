// A `ReorderableList` is a `ul` marked `data-k-list`, and an app that drags gets a base rule
// that removes the list's bullets, margin and padding, at zero specificity so the app's own
// classes still win (41-drag-and-drop.md; 07-themes-and-units.md, "Base styles").
import k.drag.DragKind
import k.drag.ReorderableList

struct Task {
    let id: String
    let title: String
}

let taskDrag: DragKind<Task> = DragKind(name: "task")

component Lists {
    @State var tasks: List<Task> = []

    view {
        ReorderableList(kind: taskDrag, id: "tasks", items: tasks, key: { task => task.id }, label: "Tasks")
            .row { task =>
                text { task.title }
            }
    }
}
