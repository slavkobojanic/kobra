/// A length of time: `200ms`, `2s`, `10.minutes`, `3.days`
/// (`docs/design/07-themes-and-units.md`, "Number extensions").
///
/// Durations need no clock; `clock.now + 3.days` is a [`Date`]. As a config
/// value it parses `30s`, `5m`, `2h` and `10.5s`.
@Native
public struct Duration implements Equatable, Hashable, Comparable, Codable, ConfigValue {
    /// The duration in whole seconds, rounded down.
    var inSeconds: Int
    /// The duration in whole milliseconds.
    var inMilliseconds: Int
}
