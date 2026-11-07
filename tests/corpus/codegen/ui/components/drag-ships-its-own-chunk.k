// `k.drag` is the runtime's drag chunk (runtime.md §12.3): `ReorderableList`, `DropTarget` and
// `DragKind(name:)` are imported from a chunk of their own, which imports the runtime chunk, and ships
// only to apps that drag; the server renders the components with `$s.natives` (codegen.md §16).
import k.drag.DragKind
import k.drag.DropTarget
import k.drag.ListMove
import k.drag.ReorderableList

struct Task {
    let id: String
    let title: String
}

let taskDrag: DragKind<Task> = DragKind(name: "task")

component Lists {
    @State var tasks: List<Task> = []
    @State var trashed: Int = 0

    func move(change: ListMove<Task>) {
        let task = tasks.remove(at: change.fromIndex)
        tasks.insert(task, at: change.toIndex)
    }

    view {
        ReorderableList(kind: taskDrag, id: "tasks", items: tasks, key: { task => task.id }, label: "Tasks")
            .row { task =>
                text { task.title }
            }
            .on(moved: move)

        DropTarget(kind: taskDrag, label: "Trash") {
            text { "Trash" }
        }
        .on(dropped: { task => trashed += 1 })
    }
}
