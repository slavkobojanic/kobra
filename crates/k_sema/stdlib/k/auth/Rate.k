/// A number of calls per period, for `RateLimit`: `100.perMinute`.
@Native
public struct Rate implements Equatable {
    /// The number of calls.
    var count: Int
    /// The period.
    var period: Duration
}

/// Rates on whole numbers.
public extend Int {
    /// This many calls per second.
    @Native
    var perSecond: Rate
    /// This many calls per minute.
    @Native
    var perMinute: Rate
    /// This many calls per hour.
    @Native
    var perHour: Rate
}
