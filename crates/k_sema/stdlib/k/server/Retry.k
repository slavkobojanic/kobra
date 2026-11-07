/// A job's retry policy: up to `attempts` runs, waiting `backoff` and then
/// doubling, with jitter. The default is 5 attempts from 10 seconds.
@Annotation
public struct Retry {
    /// The total number of runs, the first one included.
    let attempts: Int
    /// The wait before the second run; each later wait doubles.
    let backoff: Duration
}
