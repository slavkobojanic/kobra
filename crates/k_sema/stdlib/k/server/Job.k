/// A background job: a service run by the worker with a stored payload
/// (`docs/design/30-background-jobs.md`). Server-only. Each job gets a
/// generated `static let kind: JobKind<Payload>` ([`JobKind`]).
@ServerOnly
public interface Job<Payload> where Payload implements Codable {
    /// Does the work. Called only by the worker and by test code.
    ///
    /// - Parameter payload: the payload the job was enqueued with.
    /// - Throws: [`JobError.retryable`] to run it again after the backoff
    ///   ([`Retry`]), or [`JobError.permanent`] to give up.
    func run(payload: Payload) async throws JobError
}
