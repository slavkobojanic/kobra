/// Runs a [`Job`] whose payload is [`Void`] on a schedule:
/// `@Schedule(.daily(at: TimeOfDay("03:00")))`. Times are UTC.
@Annotation
public struct Schedule {
    /// Runs the job at the times `when` gives ([`ScheduleTime`]).
    @Native
    init(_ when: ScheduleTime) {}
}
