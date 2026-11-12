// The library opts in to `workers`, so its own use is fine; the app that
// depends on it must opt in too (K8109 in app/k.toml).
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

@Worker
public service Sieve {
    func echo(bytes: Bytes) async throws WorkerOnlyError -> Bytes {
        return bytes
    }
}
