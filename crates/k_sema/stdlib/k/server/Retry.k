/// A job's retry policy: up to `attempts` runs, waiting `backoff` and then
/// doubling, with jitter. The default is 5 attempts from 10 seconds.
@Annotation
public struct Retry {
    /// The total number of runs.
    let attempts: Int
    /// The first wait.
    let backoff: Duration
}
