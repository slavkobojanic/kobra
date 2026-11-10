// docs/design/55-workers.md, "Declaring a worker": a worker has no container,
// so it injects nothing (K4036); the fix removes the field.
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError
import k.time.Clock

@Worker
service Stamp {
    @Inject let clock: Clock

    func stamp(text: String, now: Date) async throws WorkerOnlyError -> String {
        return "{text} {now}"
    }
}
