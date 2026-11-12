// With `preview = ["workers"]` in k.toml the feature is used freely.
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

@Worker
public service Sieve {
    func echo(bytes: Bytes) async throws WorkerOnlyError -> Bytes {
        return bytes
    }
}
