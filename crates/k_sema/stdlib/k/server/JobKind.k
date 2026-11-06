/// The typed handle of a job, `OrderConfirmationJob.kind`, used to enqueue it
/// and to inspect it in tests.
@Native
public struct JobKind<Payload> implements Equatable, Hashable where Payload implements Codable {
    /// The job's stable name, recorded in `storage.lock`.
    var name: String
}
