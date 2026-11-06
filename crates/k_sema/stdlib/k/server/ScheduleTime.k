import k.time.TimeOfDay
import k.time.Weekday

/// When a scheduled job runs.
public enum ScheduleTime {
    /// Every `interval`.
    case every(Duration)
    /// Every day at `at`, UTC.
    case daily(at: TimeOfDay)
    /// Every week on `on` at `at`, UTC.
    case weekly(on: Weekday, at: TimeOfDay)
}
