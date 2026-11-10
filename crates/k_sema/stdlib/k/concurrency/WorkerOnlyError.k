/// The error enum of a `@Worker` method with no errors of its own:
/// `func rank(query: String) async throws WorkerOnlyError -> List<Uuid>`.
public enum WorkerOnlyError implements Error, Codable {
    /// A failure outside the method ([`WorkerFailure`]).
    case workerFailure(WorkerFailure)
}
