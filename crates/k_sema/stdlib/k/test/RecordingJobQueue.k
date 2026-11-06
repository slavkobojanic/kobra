import k.server.JobKind
import k.server.JobQueue

/// The default `JobQueue` in `k test`: it stores jobs without running them.
@TestDefault(replaces: JobQueue)
@Native
public service RecordingJobQueue implements JobQueue {
    /// Records the job.
    func enqueue<Payload>(_ kind: JobKind<Payload>, payload: Payload, delay: Duration = 0.seconds) async where Payload implements Codable
    /// The payloads enqueued for `kind`, in order.
    func payloads<Payload>(of kind: JobKind<Payload>) -> List<Payload> where Payload implements Codable
}
