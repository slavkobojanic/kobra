/// The standard [`JobQueue`], stored in the app's database (`k_job`).
@ServerOnly
@Native
public service DatabaseJobQueue implements JobQueue {
    /// Stores the job in the current transaction, so it runs only if the
    /// transaction commits; see [`JobQueue.enqueue`].
    func enqueue<Payload>(_ kind: JobKind<Payload>, payload: Payload, delay: Duration = 0.seconds) async where Payload implements Codable
}
