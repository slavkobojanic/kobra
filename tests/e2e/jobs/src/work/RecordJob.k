import k.server.Job
import k.server.JobError

/// Writes its note.
@JobName("record")
public service RecordJob implements Job<Note> {
    @Inject let entries: EntryRepository

    func run(payload: Note) async throws JobError {
        await entries.add(payload.text)
    }
}
