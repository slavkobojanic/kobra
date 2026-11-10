import k.http.HttpClient
import k.http.HttpRequest
import k.server.Job
import k.server.JobError

/// Runs every second on its own, and each run takes longer than that (it
/// waits on a slow API between counting and adding): the due times that
/// fall while it runs are skipped, since a schedule never overlaps itself,
/// so it writes one "tick" however often it is due.
@Schedule(.every(1.second))
@JobName("tick")
public service TickJob implements Job<Void> {
    @Inject let entries: EntryRepository
    @Inject let http: HttpClient
    @Inject let config: SlowConfig

    func run(payload: Void) async throws JobError {
        let ticks = await entries.count(prefix: "tick")
        let request = HttpRequest(method: .get, url: config.url.appending(path: "/slow"))

        do {
            let response = try await http.send(request: request)
        } catch _ {
        }

        if ticks == 0 {
            await entries.add("tick")
        }
    }
}
