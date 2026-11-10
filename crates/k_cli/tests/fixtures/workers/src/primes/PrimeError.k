import k.concurrency.WorkerFailure

/// Why counting primes failed.
public enum PrimeError implements Error, Codable {
    /// The count was asked for more numbers than `Primes` counts.
    case tooLarge(limit: Int)

    /// The worker crashed, or couldn't start.
    case workerFailure(WorkerFailure)
}
