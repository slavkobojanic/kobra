/// Enqueues background jobs, in the enqueuing transaction. A server-only
/// platform interface; [`DatabaseJobQueue`] is the standard one and
/// [`k.test.RecordingJobQueue`] replaces it in tests.
@Platform(side: .server)
public interface JobQueue {
    /// Stores a job to run after `delay`. Doesn't throw.
    ///
    /// - Parameters:
    ///   - kind: the job's handle, `SomeJob.kind`.
    ///   - payload: the value the job's [`Job.run`] receives.
    ///   - delay: how long to wait before the first run.
    func enqueue<Payload>(_ kind: JobKind<Payload>, payload: Payload, delay: Duration = 0.seconds) async where Payload implements Codable
}
