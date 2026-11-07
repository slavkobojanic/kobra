import k.time.TimeOfDay
import k.time.Weekday

/// When a scheduled job ([`Schedule`]) runs.
public enum ScheduleTime {
    /// Runs every time the payload duration elapses.
    case every(Duration)
    /// Runs every day at `at`, UTC.
    case daily(at: TimeOfDay)
    /// Runs every week on `on` at `at`, UTC.
    case weekly(on: Weekday, at: TimeOfDay)
}
