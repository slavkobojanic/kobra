import k.server.Job
import k.server.JobError

/// Always fails, retryably: dead after its two attempts.
@Retry(attempts: 2, backoff: 100.milliseconds)
@JobName("doomed")
public service DoomedJob implements Job<Note> {
    @Inject let entries: EntryRepository

    func run(payload: Note) async throws JobError {
        let tries = await entries.count(prefix: "doomed try")

        await entries.add("doomed try {tries + 1}")

        throw JobError.retryable(reason: "the upstream is down")
    }
}
