import k.crypto.Uuid
import k.storage.LocalStorage
import k.storage.StorageKey

/// Owns the todo list: the single source of truth that every component reads.
///
/// Components read `items` and the derived counts directly, but only this
/// service's methods can change them. Each method checks its input, applies
/// the change, and then saves the whole list to local storage, so a reload
/// always shows the latest state.
///
/// Storage failures never stop the app. The change still applies in memory,
/// and `problem` says what went wrong until the next successful save or until
/// `dismissProblem()` is called.
public service TodoService {
    @Inject let storage: LocalStorage

    /// The longest title, in characters after trimming, that `add(title:)`
    /// and `rename(id:title:)` accept.
    let maximumTitleLength: Int = 200

    // Users' saved todos live under this key. Its name and the shape of
    // `Todo` are recorded in storage.lock, so `k check` fails if a change
    // would make every saved list unreadable.
    private let savedTodos: StorageKey<List<Todo>> = StorageKey(name: "todos")

    /// Every todo, oldest first.
    @State var items: List<Todo> = []

    /// The latest storage failure, or `nil` while storage is working.
    @State var problem: StorageProblem? = nil

    /// How many todos are done.
    @Derived let completedCount: Int = items.filter { todo => todo.isDone }.count

    /// How many todos are not done yet.
    @Derived let remainingCount: Int = items.count - completedCount

    /// Whether `clearCompleted()` would remove anything.
    @Derived let hasCompleted: Bool = completedCount > 0

    /// Restores the saved todos as soon as the service is created, so the
    /// first render already shows them.
    init() {
        restore()
    }

    /// The todos that `filter` selects, oldest first.
    ///
    /// Calling this from a view or a `@Derived` initializer reads `items`, so
    /// the caller stays current as todos change.
    ///
    /// - Parameter filter: which todos to keep.
    /// - Returns: a new list. Changing it doesn't change the service.
    func visible(filter: Filter) -> List<Todo> {
        return items.filter { todo => filter.includes(todo: todo) }
    }

    /// Adds a todo that is not done yet to the end of the list, and saves.
    ///
    /// - Parameter title: the text the user typed. Leading and trailing
    ///   whitespace is removed before the title is checked and stored.
    /// - Throws: `TitleError.empty` when nothing is left after trimming, and
    ///   `TitleError.tooLong` when the trimmed title is longer than
    ///   `maximumTitleLength`. Nothing is added or saved when it throws.
    func add(title: String) throws TitleError {
        let checkedTitle = try checked(title: title)

        items.append(Todo(id: TodoId(value: Uuid.random()), title: checkedTitle, isDone: false))
        save()
    }

    /// Changes a todo's title, and saves.
    ///
    /// Does nothing when no todo has `id`, which happens when the todo was
    /// removed while it was being edited.
    ///
    /// - Parameters:
    ///   - id: the todo to rename.
    ///   - title: the new title, trimmed and checked as in `add(title:)`.
    /// - Throws: `TitleError.empty` or `TitleError.tooLong`, under the same
    ///   rules as `add(title:)`. The old title is kept when it throws.
    func rename(id: TodoId, title: String) throws TitleError {
        let checkedTitle = try checked(title: title)

        guard let index = indexOfTodo(id: id) else {
            return
        }

        items[index].title = checkedTitle
        save()
    }

    /// Marks a todo as done or not done, and saves. Does nothing when no todo
    /// has `id`.
    ///
    /// - Parameters:
    ///   - id: the todo to change.
    ///   - isDone: whether the todo is now done.
    func setDone(id: TodoId, isDone: Bool) {
        guard let index = indexOfTodo(id: id) else {
            return
        }

        items[index].isDone = isDone
        save()
    }

    /// Removes a todo, and saves. Does nothing when no todo has `id`.
    ///
    /// - Parameter id: the todo to remove.
    func remove(id: TodoId) {
        guard let index = indexOfTodo(id: id) else {
            return
        }

        items.remove(at: index)
        save()
    }

    /// Removes every todo that is done, and saves. When no todo is done it
    /// does nothing, and doesn't touch storage.
    func clearCompleted() {
        guard hasCompleted else {
            return
        }

        items.removeWhere { todo => todo.isDone }
        save()
    }

    /// Hides the current storage problem without fixing it. The next failure
    /// reports a problem again.
    func dismissProblem() {
        problem = nil
    }

    /// Replaces the in-memory list with the saved one.
    ///
    /// Runs automatically when the service is created. Call it again to pick
    /// up changes saved by another tab. When nothing has been saved yet, the
    /// list becomes empty. When the saved data can't be read, the list becomes
    /// empty too, and `problem` explains why.
    func restore() {
        do {
            items = try storage.read(key: savedTodos) ?? []
            problem = nil
        } catch .unreadable {
            items = []
            problem = .unreadable
        } catch .unavailable {
            items = []
            problem = .unavailable
        }
    }

    /// Writes the whole list to storage, and records the outcome in `problem`.
    ///
    /// Saving everything, rather than just the change, keeps the stored list
    /// identical to what the user sees, and a todo list is small enough for
    /// that to cost nothing noticeable.
    private func save() {
        do {
            try storage.write(key: savedTodos, value: items)
            problem = nil
        } catch .quotaExceeded {
            problem = .full
        } catch .unavailable {
            problem = .unavailable
        }
    }

    /// Trims `title` and checks it against the title rules.
    private func checked(title: String) throws TitleError -> String {
        let trimmed = title.trimmed()

        guard trimmed.isNotEmpty else {
            throw TitleError.empty
        }

        guard trimmed.count <= maximumTitleLength else {
            throw TitleError.tooLong(limit: maximumTitleLength)
        }

        return trimmed
    }

    /// The position of the todo with `id` in `items`, or `nil` when there is
    /// none.
    private func indexOfTodo(id: TodoId) -> Int? {
        return items.indexWhere { todo => todo.id == id }
    }
}
