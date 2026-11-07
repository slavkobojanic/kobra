/// A job's stable name for its rows in the queue:
/// `@JobName("order-confirmation")`.
@Annotation
public struct JobName {
    /// Names the job `name`, its stable name in the queue and in
    /// `storage.lock`; without it the name comes from the service's name.
    @Native
    init(_ name: String) {}
}
