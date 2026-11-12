// `@Worker` is part of the preview feature `workers`: without
// `preview = ["workers"]` in k.toml each use of the feature is K8108, with
// a fix that adds the opt-in.
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

@Worker
public service Sieve {
    func echo(bytes: Bytes) async throws WorkerOnlyError -> Bytes {
        return bytes
    }
}
