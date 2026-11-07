/// How a job run ([`Job.run`]) failed, which decides whether it is retried.
public enum JobError implements Error {
    /// The run failed for now: try again after the backoff ([`Retry`]).
    case retryable(reason: String)
    /// The run can never succeed: don't try again; the job becomes dead.
    case permanent(reason: String)
}
