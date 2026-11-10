// docs/design/55-workers.md, "Methods": every method that isn't private is a
// call into the worker, `async throws E` and not generic (K4037); the fixes
// add what is missing.
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

@Worker
service Text {
    func upper(_ text: String) -> String {
        return text.uppercased()
    }

    func lower(_ text: String) throws WorkerOnlyError -> String {
        return text.lowercased()
    }

    func first<T>(_ items: List<T>) async throws WorkerOnlyError -> T? {
        return items.first
    }

    private func helper(_ text: String) -> String {
        return text
    }
}
