import k.server.Job
import k.server.JobError

/// Runs every second on its own.
@Schedule(.every(1.second))
@JobName("tick")
public service TickJob implements Job<Void> {
    @Inject let entries: EntryRepository

    func run(payload: Void) async throws JobError {
        let ticks = await entries.count(prefix: "tick")

        if ticks == 0 {
            await entries.add("tick")
        }
    }
}
