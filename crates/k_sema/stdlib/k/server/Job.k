/// A background job: a service run by the worker with a stored payload
/// (`docs/design/30-background-jobs.md`). Server-only. Each job gets a
/// generated `static let kind: JobKind<Payload>`.
@ServerOnly
public interface Job<Payload> where Payload implements Codable {
    /// Does the work. Called only by the worker and by test code.
    func run(payload: Payload) async throws JobError
}
