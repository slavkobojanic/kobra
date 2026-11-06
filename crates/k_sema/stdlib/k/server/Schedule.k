/// Runs a `Job<Void>` on a schedule: `@Schedule(.daily(at: TimeOfDay("03:00")))`.
/// Times are UTC.
@Annotation
public struct Schedule {
    /// Sets the schedule.
    @Native
    init(_ when: ScheduleTime) {}
}
