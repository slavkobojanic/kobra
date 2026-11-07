import k.server.Job
import k.server.JobError

/// Fails permanently: never retried.
@JobName("refused")
public service RefusedJob implements Job<Note> {
    @Inject let entries: EntryRepository

    func run(payload: Note) async throws JobError {
        await entries.add("refused try")

        throw JobError.permanent(reason: "never")
    }
}
