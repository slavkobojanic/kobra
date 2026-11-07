import k.server.JobKind
import k.server.JobQueue

/// A [`JobQueue`] for tests that runs each job when the enqueuing app-level
/// transaction commits, or at once outside one: `use InlineJobQueue for
/// JobQueue`.
@Native
public service InlineJobQueue implements JobQueue {
    /// Runs the job at commit (or at once outside a transaction); a
    /// retryable failure fails the test.
    func enqueue<Payload>(_ kind: JobKind<Payload>, payload: Payload, delay: Duration = 0.seconds) async where Payload implements Codable
}
