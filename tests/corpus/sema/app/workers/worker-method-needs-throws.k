// docs/design/55-workers.md, "Methods": a method without `throws` gets a fix
// that adds `throws WorkerOnlyError` and imports it (K4037).
import k.concurrency.Worker

@Worker
service Lower {
    func lower(_ text: String) async -> String {
        return text.lowercased()
    }
}
