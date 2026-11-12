/// Runs a service in a Web Worker, on the service: `@Worker`
/// (`docs/design/55-workers.md`). Its methods are calls from the page into
/// the worker: each is `async`, takes and returns `Codable` values, and
/// throws an error enum with a `workerFailure(`[`WorkerFailure`]`)` case,
/// or [`WorkerOnlyError`]. The service keeps no state and injects nothing,
/// so the worker can be stopped and started again at any call: a cancelled
/// call terminates it, and a crash makes the call throw
/// `.workerFailure(.crashed)`. Its code ships in a chunk of its own,
/// started at the first call; `k test` runs it in-process.
@Preview("workers")
@Annotation
public struct Worker {}
