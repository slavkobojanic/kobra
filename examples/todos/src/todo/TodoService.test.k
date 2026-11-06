import k.storage.StorageKey
import k.test.MemoryStorage

test TodoService {
    @Inject let storage: MemoryStorage
    @Inject let todos: TodoService

    // Adding

    test "with nothing saved, the list starts empty" {
        expect todos.items.isEmpty
        expect todos.remainingCount == 0
        expect todos.problem == nil
    }

    test "adding trims the title and starts the todo as not done" {
        try todos.add(title: "  Buy milk  ")

        expect todos.items.count == 1
        expect todos.items[0].title == "Buy milk"
        expect not todos.items[0].isDone
    }

    test "todos keep the order they were added in, each with its own id" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")

        expect todos.items.map { todo => todo.title } == ["Buy milk", "Buy milk", "Walk the dog"]
        expect todos.items[0].id != todos.items[1].id
    }

    test "an empty or whitespace-only title is rejected" {
        expect throws TitleError.empty {
            try todos.add(title: "")
        }

        expect throws TitleError.empty {
            try todos.add(title: "   ")
        }

        expect todos.items.isEmpty
    }

    test "a title exactly at the length limit is accepted, after trimming" {
        let title = "a".repeated(times: todos.maximumTitleLength)

        try todos.add(title: "  {title}  ")

        expect todos.items[0].title == title
    }

    test "a title one character over the limit is rejected" {
        expect throws TitleError.tooLong {
            try todos.add(title: "a".repeated(times: todos.maximumTitleLength + 1))
        }

        expect todos.items.isEmpty
    }

    // Changing todos

    test "renaming trims the new title and changes nothing else" {
        try todos.add(title: "Buy milk")
        let original = todos.items[0]

        try todos.rename(id: original.id, title: " Buy oat milk ")

        expect todos.items[0].title == "Buy oat milk"
        expect todos.items[0].id == original.id
        expect todos.items[0].isDone == original.isDone
    }

    test "renaming to a blank title throws and keeps the old title" {
        try todos.add(title: "Buy milk")

        expect throws TitleError.empty {
            try todos.rename(id: todos.items[0].id, title: "   ")
        }

        expect todos.items[0].title == "Buy milk"
    }

    test "changing a todo that was removed does nothing" {
        try todos.add(title: "Buy milk")
        let removed = todos.items[0].id
        todos.remove(id: removed)

        try todos.rename(id: removed, title: "Buy oat milk")
        todos.setDone(id: removed, isDone: true)
        todos.remove(id: removed)

        expect todos.items.isEmpty
    }

    test "marking todos done and not done updates the counts" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")

        todos.setDone(id: todos.items[0].id, isDone: true)

        expect todos.items[0].isDone
        expect todos.completedCount == 1
        expect todos.remainingCount == 1
        expect todos.hasCompleted

        todos.setDone(id: todos.items[0].id, isDone: false)

        expect todos.completedCount == 0
        expect not todos.hasCompleted
    }

    test "removing a todo keeps the others in order" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")
        try todos.add(title: "Call Ana")

        todos.remove(id: todos.items[1].id)

        expect todos.items.map { todo => todo.title } == ["Buy milk", "Call Ana"]
    }

    test "clearing completed removes only the done todos" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")
        todos.setDone(id: todos.items[0].id, isDone: true)

        todos.clearCompleted()

        expect todos.items.map { todo => todo.title } == ["Walk the dog"]
    }

    test "clearing completed with nothing done doesn't write to storage" {
        try todos.add(title: "Buy milk")
        storage.failWrites(error: .quotaExceeded)

        todos.clearCompleted()

        expect todos.problem == nil
    }

    test "each filter selects the matching todos" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")
        todos.setDone(id: todos.items[1].id, isDone: true)

        expect todos.visible(filter: .all).count == 2
        expect todos.visible(filter: .active).map { todo => todo.title } == ["Buy milk"]
        expect todos.visible(filter: .completed).map { todo => todo.title } == ["Walk the dog"]
    }

    // Saving and restoring

    test "every change is saved" {
        try todos.add(title: "Buy milk")
        try todos.add(title: "Walk the dog")
        todos.setDone(id: todos.items[0].id, isDone: true)
        todos.remove(id: todos.items[1].id)

        let saved = try storage.read(key: savedTodosKey)

        expect saved == todos.items
    }

    test "restoring loads the saved list" {
        try storage.write(key: savedTodosKey, value: [Todo.sample(title: "Buy milk", isDone: true)])

        todos.restore()

        expect todos.items.map { todo => todo.title } == ["Buy milk"]
        expect todos.completedCount == 1
    }

    test "unreadable saved data starts an empty list and says so" {
        storage.writeRaw(name: "todos", text: "{{ this is not JSON")

        todos.restore()

        expect todos.items.isEmpty
        expect todos.problem == .unreadable
    }

    test "the next successful save replaces unreadable data" {
        storage.writeRaw(name: "todos", text: "{{ this is not JSON")
        todos.restore()

        try todos.add(title: "Buy milk")
        let saved = try storage.read(key: savedTodosKey)

        expect todos.problem == nil
        expect saved == todos.items
    }

    test "blocked storage is reported when restoring" {
        storage.failReads(error: .unavailable)

        todos.restore()

        expect todos.items.isEmpty
        expect todos.problem == .unavailable
    }

    test "a full storage keeps the change in memory and reports it" {
        storage.failWrites(error: .quotaExceeded)

        try todos.add(title: "Buy milk")
        storage.stopFailing()
        let saved = try storage.read(key: savedTodosKey)

        expect todos.items.count == 1
        expect todos.problem == .full
        expect saved == nil
    }

    test "a successful save clears the problem and catches up on missed changes" {
        storage.failWrites(error: .quotaExceeded)
        try todos.add(title: "Buy milk")
        storage.stopFailing()

        try todos.add(title: "Walk the dog")
        let saved = try storage.read(key: savedTodosKey)

        expect todos.problem == nil
        expect saved == todos.items
    }

    test "a dismissed problem stays hidden until the next failure" {
        storage.failWrites(error: .unavailable)
        try todos.add(title: "Buy milk")

        todos.dismissProblem()

        expect todos.problem == nil

        try todos.add(title: "Walk the dog")

        expect todos.problem == .unavailable
    }
}

/// The key `TodoService` saves under. The tests spell it out instead of
/// asking the service, because it is part of the app's stored format: if it
/// changed, users would lose their saved todos, and these tests should fail.
/// Test code isn't recorded in storage.lock, so it may declare a key with the
/// same name as the service's.
let savedTodosKey: StorageKey<List<Todo>> = StorageKey(name: "todos")
