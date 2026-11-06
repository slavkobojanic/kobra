/// A job's stable name for its rows in the queue:
/// `@JobName("order-confirmation")`.
@Annotation
public struct JobName {
    /// Names the job.
    @Native
    init(_ name: String) {}
}
