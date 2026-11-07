import k.server.Job
import k.server.JobError

/// Fails its first attempt, then succeeds.
@Retry(attempts: 3, backoff: 100.milliseconds)
@JobName("flaky")
public service FlakyJob implements Job<Note> {
    @Inject let entries: EntryRepository

    func run(payload: Note) async throws JobError {
        let tries = await entries.count(prefix: "flaky try")

        await entries.add("flaky try {tries + 1}")

        guard tries >= 1 else {
            throw JobError.retryable(reason: "not yet")
        }

        await entries.add("flaky done {payload.text}")
    }
}
