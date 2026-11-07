// Two reorderable lists of one kind (41-drag-and-drop.md): the payload type comes from the kind, the
// chained scoped `row` slot draws each item, and `moved` delivers a `ListMove` of that type.
import k.drag.DragKind
import k.drag.ListMove
import k.drag.ReorderableList

struct Task implements Equatable {
    let id: String
    let title: String
}

let taskDrag: DragKind<Task> = DragKind(name: "task")

component Lists {
    @State var todo: List<Task> = []
    @State var done: List<Task> = []

    func move(change: ListMove<Task>) {
        let moving = change.item
        //  ^^^^^^ type: corpus.Task
        if change.fromList == "todo" {
            todo.remove(at: change.fromIndex)
        } else {
            done.remove(at: change.fromIndex)
        }

        if change.toList == "todo" {
            todo.insert(moving, at: change.toIndex)
        } else {
            done.insert(moving, at: change.toIndex)
        }
    }

    view {
        ReorderableList(kind: taskDrag, id: "todo", items: todo, key: { task => task.id }, label: "To do")
            .row { task =>
                text { task.title }
            }
            .itemLabel { task => task.title }
            .on(moved: move)

        ReorderableList(kind: taskDrag, id: "done", items: done, key: { task => task.id }, label: "Done")
            .row { task =>
                text { task.title }
            }
            .accepts { task => task.title.isNotEmpty }
            .on(moved: { change => move(change: change) })
    }
}
