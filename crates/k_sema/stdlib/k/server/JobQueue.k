/// Enqueues background jobs, in the enqueuing transaction. A server-only
/// platform interface; `RecordingJobQueue` replaces it in tests.
@Platform(side: .server)
public interface JobQueue {
    /// Stores a job to run after `delay`. Doesn't throw.
    func enqueue<Payload>(_ kind: JobKind<Payload>, payload: Payload, delay: Duration = 0.seconds) async where Payload implements Codable
}
