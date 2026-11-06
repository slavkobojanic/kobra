/// How a job run failed, which decides whether it is retried.
public enum JobError implements Error {
    /// Try again after the backoff.
    case retryable(reason: String)
    /// Never try again; the job becomes dead.
    case permanent(reason: String)
}
