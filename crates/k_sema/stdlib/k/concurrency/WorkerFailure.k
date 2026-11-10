/// A failure outside a `@Worker` service's method, carried by the
/// `workerFailure` case every worker method's error enum has
/// (`docs/design/55-workers.md`, "Errors and crashes"). Only the runtime
/// produces it.
public enum WorkerFailure implements Error, Codable {
    /// The worker stopped while it ran the call: the method panicked or
    /// threw something that isn't a K error, or the browser ended it. The
    /// next call starts a new worker.
    case crashed
    /// The browser couldn't start the worker: it has no `Worker`, or the
    /// worker's script failed to load (refused by a content security
    /// policy, or unreachable offline). The next call tries again.
    case unavailable
}
