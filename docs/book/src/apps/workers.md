# Work off the main thread

A page that computes something heavy while the user types or scrolls stutters, because the browser runs
the page's code and paints on one thread. Mark a service **`@Worker`** (from `k.concurrency`) and its
methods run in a Web Worker instead: the page awaits them like any async call, and keeps painting
meanwhile. The design is in `docs/design/55-workers.md`.

```k
// examples/notes/src/notes/NoteSearch.k
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError
import k.crypto.Uuid
// ...
@Worker
public service NoteSearch {
// ...
    func rank(notes: List<NoteSummary>, query: String) async throws WorkerOnlyError -> List<Uuid> {
        let words = query.lowercased().split(separator: " ").filter { word => word.isNotEmpty }
        let scored = notes.compactMap { note => scoredHit(note, words) }
        return scored.sortedByDescending { hit => hit.points }.map { hit => hit.id }
    }
// ...
}
```

The notes list injects it like any service, and ranks the filter from an effect:

```k
// examples/notes/src/notes/Notes.k
    @Inject let notes: NotesService
    @Inject let search: NoteSearch
// ...
    @Effect(on: [filter, notes.all])
    func rank() async {
        let query = filter
        guard query.trimmed().isNotEmpty else {
            ranking = nil
            return
        }
        do {
            let ids = try await search.rank(notes: notes.all, query: query)
            ranking = Ranking(query: query, ids: ids)
        } catch .workerFailure {
            ranking = nil
        }
    }
```

## What a worker service is

- **It computes; it keeps nothing.** Its inputs are its arguments and its output is its result. It has no
  `@State`, `@Derived` or `var` fields, no `init()` and injects nothing (K4035, K4036): the worker can be
  stopped and started again at any call, so there must be nothing to lose. Its `let` constants, computed
  properties and helper functions are `private` (the page can't read them, they live in the worker); the
  quick fix of K4035 makes a constant `private`.
- **Each method that isn't `private` is a call into the worker.** It is `async`, and `throws` an error
  enum with a `workerFailure(WorkerFailure)` case, or `WorkerOnlyError` when it has no errors of its own,
  just as every rpc's error has `rpcFailure(RpcFailure)` (K4037, K4038; the quick fixes add `async`,
  `throws WorkerOnlyError` or the missing case). Its parameters, result and error are `Codable` (K4039):
  they cross to the other thread through their codecs, as an rpc's do, so a `Date` or a struct arrives as
  itself.
- **It runs in the browser only.** Calling it from code that runs while the server renders (a `@Load`, a
  field's initializer, a view) is error K4012; call it from an `@Effect` or an event handler.

```k,standalone,fails=K4035+K4036
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError
import k.time.Clock

@Worker
service Totals {
    @Inject let clock: Clock         // K4036: a worker injects nothing
    @State var lastTotal: Int = 0    // K4035: a worker keeps no state

    func sum(of numbers: List<Int>) async throws WorkerOnlyError -> Int {
        return numbers.sum()
    }
}
```

## How calls run

- **One worker per service,** started at the first call: a page that never calls it doesn't load it.
  Calls run one at a time, in the order they were made.
- **Cancelling a call stops the work.** A call made from an effect, a load or a handler is cancelled
  when its owner is: the effect runs again, or the page is left. A call still waiting its turn is dropped;
  the running one terminates the worker, and the next call starts a new one. That is why the notes filter
  can rank on every key: typing on stops the stale search.
- **Failures are typed.** The method's own errors arrive as thrown. A panic in the worker (a bug), or the
  browser ending the worker, is `.workerFailure(.crashed)`, and the next call starts a new worker; a
  worker the browser can't start at all is `.workerFailure(.unavailable)`. The notes list falls back to
  its plain substring filter in both cases.
- **`Bytes` crosses as a copy,** without being encoded as numbers, when it is a parameter or the result.

## Where its code goes

The service's code ships in a file of its own, `/assets/workers/<name>.<hash>.js`, with only the
declarations it reaches: the notes search worker holds `NoteSearch`, `NoteSummary` and its codec, not the
app's pages. The page has a small stand-in of the same name that sends the calls. The worker's file is
on the app's own origin, so a content security policy needs only `worker-src 'self'`, and an `@Offline`
app ([Offline and installable apps](offline.md)) caches it with the rest of the build.

## Tests

`k test` runs a worker service in the test's own thread, through the same codecs and the same queue, so a
test calls it, and a page that calls it, exactly as the browser does. A panic in it is
`.workerFailure(.crashed)` there too. Wait for a page's call with `await screen.settle()`, as for an rpc.

```k
// examples/notes/src/notes/NoteSearch.test.k
test NoteSearch {
    @Inject let search: NoteSearch

    test "a word in the title ranks above one in a tag, and a tag above the text" {
        let inText = note(Uuid("00000000-0000-4000-8000-000000000001"), title: "Groceries", body: "Plan the trip shopping")
        let inTag = note(Uuid("00000000-0000-4000-8000-000000000002"), title: "Packing", tags: ["trip"])
        let inTitle = note(Uuid("00000000-0000-4000-8000-000000000003"), title: "Trip to Lisbon")

        let ids = try await search.rank(notes: [inText, inTag, inTitle], query: "Trip")

        expect ids == [inTitle.id, inTag.id, inText.id]
    }
// ...
}
```
