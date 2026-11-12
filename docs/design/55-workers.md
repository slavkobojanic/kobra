# Workers

## Decision

**A service marked `@Worker` runs in a Web Worker.** Its methods are the calls the page makes to it: `async`, `Codable` in and out, with a typed error that includes the worker's own failures. The page injects it like any service and awaits its methods; the main thread holds a small proxy, and the service's code, with only what it reaches, ships in a chunk of its own that the browser starts as a module worker at the first call. In `k test` the same proxy calls the service in-process. This is Tier 2 of [45's inventory](45-web-platform.md#inventory): "K-typed workers (`k.concurrency`)".

**Workers are a preview feature** (`workers` in `k_sema::features`, [48](48-releases-and-versioning.md#stability-levels)): `@Worker`, `WorkerFailure` and `WorkerOnlyError` are marked `@Preview("workers")`, and a project lists `preview = ["workers"]` in `[package]` to use them (K8108 otherwise). Decided 2026-11-12, as the newest built feature, the one whose API is likeliest to change before it is stable.

```k
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError
import k.crypto.Uuid

/// Ranks notes for a search off the main thread, so typing stays smooth
/// with thousands of notes.
@Worker
public service NoteSearch {
    func rank(notes: List<NoteSummary>, query: String) async throws WorkerOnlyError -> List<Uuid> {
        let words = query.lowercased().split(separator: " ").filter { word => word.isNotEmpty }
        return notes.compactMap { note => scoredHit(note, words) }
            .sortedByDescending { hit => hit.points }
            .map { hit => hit.id }
    }

    private func scoredHit(_ note: NoteSummary, _ words: List<String>) -> SearchHit? { … }
}
```

```k
public page Notes requires Authenticated at "/" {
    @Inject let notes: NotesService
    @Inject let search: NoteSearch

    @State var filter: String = ""
    @State var ranking: Ranking? = nil

    @Derived let visible: List<NoteSummary> = if ranking?.query == filter { notes.ordered(ids: ranking?.ids ?? []) } else { notes.matching(filter: filter) }

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
            ranking = nil    // the page's own substring filter
        }
    }
}
```

Four rules hold everywhere:

- **A worker computes; it holds no state.** Its inputs are its arguments, its output its result. No `@State`, no `var`, no `@Inject`: a worker can be stopped and started again at any call (cancellation, a crash), and nothing the page relies on is lost.
- **Everything crosses as a value.** Arguments, results and errors are `Codable` and cross through their codecs, exactly as an rpc's do ([28](28-rpc-wire-format.md)); the page and the worker never share an object.
- **One worker per service, one call at a time.** Calls queue in the page, in order; the worker runs the first and gets the next when it answers.
- **A failure of the worker is a typed error, never a panic in the page:** every method's error enum has the case `workerFailure(WorkerFailure)`, as every rpc's has `rpcFailure(RpcFailure)` ([28](28-rpc-wire-format.md#network-failures)).

## Declaring a worker

- **`@Worker` (`k.concurrency.Worker`, imported; no arguments) goes on a `service` only;** elsewhere it is K3002 with a fix that removes it.
- **The service is client-only** ([10](10-services.md#serverclient-boundary)): server-only code that injects it is K4008, and calling its methods from code that can run while the server renders is K4012 (below, [Server rendering](#server-rendering)).
- **Members it may have:** instance `func`s, which are the calls, and `private` members of any other kind: `let` constants with an initializer (evaluated once in each worker it starts), computed properties, helper functions, `static` ones included. The page sees only the calls.
- **Members it may not have,** each K4035 (`worker-member-not-allowed`) at the member: `@State`, `@Derived`, a `var` field, a `let` without an initializer, a field or computed property that isn't `private` (the page would read it, but it lives in the worker; the fix makes it `private`), `init()`, `@Effect`, `@Scope(…)` on the service (whose fix removes it), an `rpc`, and a non-private `static` function (which the page would run itself, on the main thread). The message says why: the worker can be restarted at any call, so it keeps no state; state belongs to a service on the main thread that calls the worker.
- **It injects nothing** (K4036, `worker-injection`, at the field, with a fix that removes it). A worker has no container: what it needs is an argument, as for a plain function, which takes `now: Date` rather than injecting `Clock` ([35](35-time.md)). Rpcs, storage and the DOM belong to the main thread's services.
- **Who injects it:** components, pages, layouts and client services, like any service ([10](10-services.md#layering)). It is in `.app` scope: one per app, hence one worker per tab.

## Methods

Every instance method that isn't `private` is a call into the worker:

```k
func histogram(samples: Bytes, buckets: Int) async throws WaveError -> List<Double>
```

- **`async`, and `throws E`:** a missing `async` or `throws` is K4037 (`worker-method-signature`), with fixes that add `async` and `throws WorkerOnlyError`. A generic method is K4037 too: codecs need the concrete types.
- **`E` is `WorkerOnlyError` or a `Codable` error enum with exactly one `workerFailure(WorkerFailure)` case,** else K4038 (`worker-error-without-failure-case`), whose fix adds the case, as K5502's does for rpcs.
- **Parameters, the result and `E` are `Codable`** (K4039, `worker-payload-not-codable`). A `Stream<T>` result is K4039 too, with a note: streamed progress is an [open question](#open-questions).
- **Inside the worker,** a method's code is ordinary K; it calls the service's other members and any top-level function, struct or enum. Its typed errors are thrown as usual.

```k
/// Why a worker call failed outside the method (only the runtime produces it).
public enum WorkerFailure implements Error, Codable {
    /// The worker stopped while it ran the call: a panic, an exception the
    /// method didn't declare, or the browser ended it (out of memory). The
    /// next call starts a new worker.
    case crashed
    /// The browser couldn't start the worker: no `Worker` in this browser, or
    /// its script failed to load (a content security policy that refuses
    /// it, or the chunk unreachable offline in an app without `@Offline`).
    case unavailable
}

/// The error enum of a worker method with no errors of its own.
public enum WorkerOnlyError implements Error, Codable {
    case workerFailure(WorkerFailure)
}
```

## Calls

- **Values cross through their codecs.** The proxy encodes each argument with its codec into JSON values, posts them (`postMessage`, a structured clone of plain JSON: no class instance, function or signal crosses), the worker decodes them with the same codecs, and the result and a thrown error come back the same way. So a `Date`, a `Money` or a struct with an `@JsonKey` arrives as itself, and a decoding failure can't happen between two halves of one build.
- **`Bytes` at the top level isn't encoded:** a `Bytes` parameter or result is posted as its `Uint8Array`, which structured clone copies in one block, instead of the codec's list of numbers. It is copied, not transferred: K values are immutable and the caller still holds it, and a transfer would empty it. `Bytes` inside a struct goes through the struct's codec.
- **The queue.** One worker per service; calls wait in the page in the order they were made, and the worker runs one at a time. A worker is one thread, so running two calls "at once" would only interleave them; queuing in the page is what lets a cancelled call that hasn't started be dropped without disturbing the running one.
- **Lifetime.** The worker starts at the first call, not when the service is injected, so a page that never calls it costs nothing; it is a module worker (`new Worker(url, {type: "module", name: "NoteSearch"})`), and its first call waits for it to load. It then stays for the life of the page. There is no idle timeout ([open question](#open-questions)).

### Cancellation

A call made from an effect, a load or a handler carries its owner's cancellation ([03](03-reactivity.md#async-work-is-owned)), and a cancelled `await` never resumes ([codegen.md §8](../compiler/codegen.md#8-async-and-cancellation)):

- **A queued call that is cancelled leaves the queue;** the worker never sees it.
- **A running call that is cancelled terminates the worker** (`worker.terminate()`): JavaScript has no other way to stop a synchronous computation, and a call that runs on after nobody waits for it would delay every call behind it. The call never settles. The next call in the queue starts a new worker.
- Because the worker holds no state, the only visible effect of a restart is the time it takes (a module load and the service's constants). Typing into a search field whose effect re-runs on each key therefore stops a stale search as soon as the next key arrives.

### Errors and crashes

| What happens in the worker | What the call throws | The worker |
|---|---|---|
| the method throws a case of `E` | that case, decoded with `E`'s codec | keeps running |
| the method panics, or throws something that isn't a K error (a bug in interop) | `.workerFailure(.crashed)` | terminated; the next call starts a new one |
| the browser ends it (`error` event, out of memory), or a message can't be read (`messageerror`) | `.workerFailure(.crashed)` for the running call | started again for the queued ones |
| it can't start: no `Worker`, its script fails to load, a CSP refuses it | `.workerFailure(.unavailable)` for every queued call | tried again at the next call |

- **A worker announces itself** once its module has run (`{k: "ready"}`): an error before that is a load failure (`.unavailable`), after it a crash. The proxy posts the first call only once the worker is ready.
- **A crash is logged** in the console with the worker's message (a panic's text), in development with the K stack the worker's source map gives.
- **Only the runtime produces `workerFailure`.** A method that throws it itself isn't checked in this round ([open questions](#open-questions)).

## Server rendering

**A worker never runs on the server.** Calling a `@Worker` service's method from code that can run while the server renders (a field initializer, a `@Derived` or `@Load` initializer, a view) is K4012, the existing error for client-only interfaces; effects, handlers and `init()` call it freely, since they run only in the browser. Inside a client-rendered component ([47](47-client-rendered-components.md)) or a client-rendered app, everything runs in the browser and K4012 doesn't apply.

The server's container still builds the service, like any client-only service, so a server-rendered page that injects it renders; nothing calls it there.

**Decision: a compile error, not running the method inline.** Inline on the server, a CPU-heavy method would block the server's event loop for every request in flight, which is the opposite of what the annotation asks for; and its result would be computed for a page whose browser can compute it without the round trip. A page that wants the answer in its first paint computes it with a plain function, or with an rpc.

## Tests

**`k test` runs a worker inline:** the same proxy, the same codecs and the same queue, with an in-process host instead of a `Worker`. A call is dispatched in a microtask, the method runs on the test's thread, and its result, error or panic comes back through the codecs as in the browser. So a test of a component that awaits a worker sees exactly what the browser would, minus the thread:

```k
test NoteSearch {
    @Inject let search: NoteSearch

    test "a word in the title ranks above one in a tag, and a tag above the text" {
        let inText = note(Uuid("00000000-0000-4000-8000-000000000001"), title: "Groceries", body: "Plan the trip shopping")
        let inTag = note(Uuid("00000000-0000-4000-8000-000000000002"), title: "Packing", tags: ["trip"])
        let inTitle = note(Uuid("00000000-0000-4000-8000-000000000003"), title: "Trip to Lisbon")

        let ids = try await search.rank(notes: [inText, inTag, inTitle], query: "Trip")

        expect ids == [inTitle.id, inTag.id, inText.id]
    }
}
```

- **Deterministic:** no thread, no timer. Everything a call does has happened by the time the test's `await` returns, and a component's effect or handler that calls a worker has settled after `await screen.settle()`, as for an rpc.
- **A panic in the method is `.workerFailure(.crashed)`,** as in the browser, so the fallback path can be tested by giving the method an input it panics on.
- **Cancellation** drops a queued call; a running one can't be stopped on one thread, so it runs to the end and its result is dropped. The test sees what the browser shows: the call never settles.
- **A fake** is bound like any service's: a test-only service implementing the worker service (`use FakeSearch for NoteSearch`) replaces the proxy, and then nothing is encoded.

## Bundles

- **The worker chunk** `workers/<name>.<hash>.js` (`workers/note-search.3f2a1b9c.js`) holds the service's class, the declarations it reaches (structs, enums, functions, their codecs) and a few lines that serve its methods; it imports what it uses from the runtime chunk and `serveWorker` from the `workers` chunk. It never imports the shared chunk: that holds the app's components, which a worker has no use for. It is linked and minified with the rest of a release build, and named after its bytes, so a change to the worker's code changes its name, the proxy's text and hence the shared chunk's name.
- **The proxy** is the service's name in the shared chunk, a class whose methods encode, post and decode, so the containers construct it as before. It calls `workerClient`, in the runtime feature chunk `workers.<hash>.js` (`runtime/workers.js`), which ships only to apps with a `@Worker` service, so **the runtime chunk doesn't grow.**
- **The worker is started from the proxy's own URL:** `new Worker(new URL("./workers/note-search.<hash>.js", import.meta.url), {type: "module", name})`. Same origin, so a content security policy needs `worker-src 'self'` (or `script-src 'self'`, which it falls back to); no `blob:` URL and no `eval`. Module workers are in every browser K supports (Baseline 2023).
- **`@Offline`** precaches the worker chunks with the other files below `/assets/` ([54](54-offline.md#what-is-precached)), so a worker starts offline.
- **`k dev`** serves the worker chunk with a source map like any other, so DevTools shows the worker's `.k` source and stops at its breakpoints.

## Examples

**Notes ranks its filter in a worker** (`examples/notes`, `NoteSearch`). Of the examples it is the one whose work grows with the user's data on every key press: the list's filter scores every note of the account, and with thousands of notes that would make typing stutter. The page shows the plain substring matches at once and swaps in the worker's ranking (a word in the title above one in a tag, above one in the text) when it arrives for what was typed; typing on cancels the stale search, which stops the worker; and without a worker (`.workerFailure`) the plain matches stay. Weather's and memos' work is either the server's (the forecast) or the browser's own (audio encoding), so they gain nothing from a worker.

**The `workers` fixture** (`crates/k_cli/tests/fixtures/workers`) is a server-rendered page that counts primes by trial division, slow on purpose, for the real-Chrome tests and `k test`.

## Tests of the feature

- **Sema corpus** `sema/app/workers/`: a clean worker, K4035–K4039 with their fixes, K3002 on a struct, and K4012 and K4008 at the boundary.
- **Codegen:** `codegen/split/workers-ship-their-own-chunk` and `crates/k_codegen/tests/codegen.rs` (the chunk holds only what the service reaches and no shared code, the proxy and its codecs, the server's empty class, the linked name, the in-process host of a test bundle, every profile binds the worker's stdlib types); `link.rs`'s unit test of a chunk named by `new URL(…)`.
- **`workers.test.js`** against a scripted `Worker`: start at the first call, ready before posting, one call at a time in order, typed errors, cancelling a queued and a running call, an aborted signal, a crash and the restart, an error before and after ready, no `Worker`, an undecodable answer; and `serveWorker` against a scripted scope.
- **`k test`** on the fixture (results, a typed error, `Bytes`, a panic as `.crashed`, the restart, a page awaiting the worker) and on notes (`NoteSearch.test.k`, the `Notes` page's ranking).
- **Real Chrome** (`chrome.rs`): `requestAnimationFrame` keeps ticking (at least 20 frames a second) while the worker counts, stopping a long count lets the next count answer within 5 s from a new worker, a typed error arrives with its payload, a crash is `.crashed` and the next call works, and notes ranks its filter through `/assets/workers/note-search.<hash>.js`, each with the release client and (`K_CHROME_PROFILE=development`) the development one.

## As built

Built tests first, as designed above, with these decisions:

- **Declarations.** `k.concurrency.Worker` (an `@Annotation` struct, target `Service`), `WorkerFailure` and `WorkerOnlyError` (`Codable` enums). The side of a `@Worker` service is client-only in `di::compute_side`; K4012's check (`body.rs`) covers a `@Worker` service's methods besides the stdlib's client-only interfaces, with a message of its own, and skips a worker's calls to itself. The checks are `k_sema::workers`.
- **Codegen.** `workers.rs` in `k_codegen`: on the client a `@Worker` service lowers to its proxy, `class S { constructor() { this.$w = $k.workerClient(open, "S"); } async m($c, …) { return this.$w.call($c, "m", [encoded…], decode, decodeError, failure); } }`; the worker chunk is built from a second lowering of the program in which the service is its real class and its host table, `S$host = { m: [decodeArguments, encodeResult, encodeError] }`, kept to what the service reaches, the service last. The stdlib declarations of that lowering are taken from what the codecs use too (`WorkerOnlyError`'s codec needs `WorkerFailure`'s). The proxy names the chunk with a placeholder that the bundler replaces in the syntax tree once the chunk is printed, and the linker orders a chunk named by `new URL(…)` before the chunk that names it (`Imports::urls`). A test bundle wraps the class and its table in `S$worker`, so the class's own name inside it is the real class's.
- **Runtime.** `runtime/workers.js` (part `Workers`, a runtime feature chunk like `forms`, `WORKERS_SIZE_BUDGET` 1.25 KB; 1,060 B compacted and gzipped as built): `workerClient(open, name)` with the queue, cancellation and failures, and `serveWorker(scope, make, host)`, which the worker chunk calls with `globalThis`. `inlineWorker(make, host)`, the in-process host `k test` uses, is the test kit's (`testing/kit.js`), so no app ships it. The message protocol: `{k: "call", id, m, a}` to the worker; `{k: "ready"}`, `{k: "ok", id, v}`, `{k: "error", id, e}` and `{k: "crash", id, text}` back. A thrown value that isn't an `Error` (a K enum value) is a typed error; anything else, a `KPanic` included, a crash.
- **Sizes.** The runtime chunk doesn't change. Notes ships 41,443 B gzip, 2,930 B more than before: the `workers` chunk (1,008 B linked), its worker chunk (1,357 B, fetched at the first search) and the proxy and the page's ranking; **notes' budget rose from 40,000 B to 43,500 B** (performance.md §11.7), and `WORKERS_SIZE_BUDGET` (1.25 KB) is new.

## Why

- **A service, not a new kind of declaration:** a worker is a set of operations the page calls, which is what a service already is in K; injecting it, faking it in tests and finding it in the code work as for every service.
- **No state:** a worker that can be terminated to cancel a call and restarted after a crash can't promise to keep anything. Making that a rule instead of a caveat means cancellation and crashes never lose data, and the main thread's services stay the one place state lives.
- **Typed failures, like rpcs:** a worker is another thread that can die independently of the page, which is the same situation as a server that can be unreachable. The `workerFailure` case makes the page handle it where it already handles errors, with an exhaustive `catch`.
- **Codecs, not structured clone of K values:** K values are partly runtime classes (`Date`, `Money`, hash maps keyed by structs) that structured clone would turn into plain objects. The codecs already define one form for every `Codable` type, and are what an rpc uses.
- **A chunk with only what the service reaches:** the worker starts at the first call, so its download is on that call's path; the app's components and the shared chunk would only make it slower.
- **Terminating on cancel:** a stale CPU-bound call can't be interrupted any other way, and since nothing is lost, stopping it is free apart from the restart.

## Alternatives rejected

- **Closures sent to a worker** (`spawn { … }`, Comlink's proxies of arbitrary objects): a closure captures the page's state, which can't cross threads; proxied objects make every property read a message. Declared methods with `Codable` signatures are checkable at compile time.
- **Stateful workers** (an index built once and queried many times): see [Why](#why). A page that needs to reuse a large input passes it each call, or keeps the derived data itself (the worker can return it); a cache that survives restarts is an [open question](#open-questions).
- **Running inline when `Worker` is missing:** it would freeze the page exactly when the work is heavy; `.unavailable` lets the page choose its own fallback (notes filters by substring instead).
- **Running inline during server rendering:** see [Server rendering](#server-rendering).
- **Transferring `Bytes`:** a transfer empties the sender's buffer, and a K value the caller still holds must not change.
- **A `blob:` URL or an inline script for the worker:** content security policies commonly refuse `blob:` workers; a same-origin chunk needs nothing.
- **A pool of workers per service,** or parallel calls: one queue is predictable and enough for keeping the page responsive; parallel speed-ups are an [open question](#open-questions).
- **Shared memory and `Atomics`, `SharedWorker`, workers on the server:** out of scope (shared memory needs cross-origin isolation headers on every response; a `SharedWorker` would need state, which workers don't have; the server has background jobs, [30](30-background-jobs.md)).

## Open questions

- **Progress: `Stream<T>` results.** A method returning `Stream<Progress>` would post each element as it is produced; the stream's end and cancellation map onto the same protocol. Not in this round.
- **An idle timeout.** A worker that hasn't been called for a while could be terminated to free its memory, at the price of a restart on the next call.
- **Pools** (`@Worker(count: 4)`) for parallel independent calls.
- **Transferring results:** a `Bytes` result the worker doesn't keep could be transferred rather than copied, since the worker forgets it; worth measuring with large buffers.
- **A worker injecting pure stdlib services** (`Clock`'s `now` in a worker): today it takes them as arguments.
- **Checking `workerFailure`:** a method that throws `.workerFailure(…)` itself, which only the runtime should produce, isn't an error yet (K5506 is, for an rpc's handler).
