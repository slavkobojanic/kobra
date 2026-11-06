# Proposal: Background Jobs (`k.server`)

> **Status: proposal.** Not yet accepted into the design. First used by `examples/shop`.

## Problem

[20-standard-library.md](../20-standard-library.md) lists "background jobs, cron schedules, queues" in `k.server`, and [13-tooling.md](../13-tooling.md) mentions long-running jobs, but no API is defined: how a job is declared, how work is enqueued, what happens on failure, where the queue lives, and how tests observe jobs.

## Proposal

### A job is a service that implements `Job<Payload>`

```k
@Retry(attempts: 8, backoff: 30.seconds)
service OrderConfirmationJob implements Job<OrderConfirmation> {
    @Inject let orders: OrderRepository
    @Inject let mailer: EmailProvider

    func run(payload: OrderConfirmation) async throws JobError {
        guard let order = await orders.byId(id: payload.orderId) else {
            throw JobError.permanent(reason: "Order {payload.orderId} no longer exists")
        }

        …
    }
}

struct OrderConfirmation implements Codable {
    let orderId: Uuid
}
```

```k
public interface Job<Payload> where Payload implements Codable {
    func run(payload: Payload) async throws JobError

    /// Default method: stores the payload for the worker to run later.
    func enqueue(payload: Payload, delay: Duration = 0.seconds) async { … }
}

public enum JobError implements Error {
    case retryable(reason: String)    // try again after the backoff
    case permanent(reason: String)    // never try again
}
```

- **No new keyword.** A job is a service with an interface, so injection, scopes and fakes work as for any service. `Job` is server-only, so a service that implements it is server-only too.
- **The payload is `Codable`,** because it is stored. Prefer ids over copies of data: the job then reads current data when it runs, and no personal data sits in the queue. A payload that contains a `Secret` is a compile error ([config-and-secrets.md](config-and-secrets.md)).
- **Enqueueing is a call on the injected job:** `await confirmations.enqueue(payload: …)`. Calling `run` directly runs it inline, which is what tests of the job itself do.
- `enqueue` doesn't throw. Failing to store a job is an infrastructure failure, which panics like other database failures ([repository-writes.md](repository-writes.md)).

### Retries

- **`@Retry(attempts:backoff:)`** on the job sets the policy: up to `attempts` runs in total, waiting `backoff`, then doubling each time, with jitter. Without it, the default is 5 attempts from 10 seconds.
- `JobError.retryable` and a panic during `run` count as failed attempts. `JobError.permanent` stops at once.
- A job that runs out of attempts, or fails permanently, is kept as **dead** with its last reason and logged at error level through `k.observe`. Dead jobs are never retried automatically.
- **Delivery is at least once.** If the process dies after the work but before the job is acknowledged, it runs again, so a job must be idempotent or accept rare duplicates (an extra confirmation email, for example).

### The queue

- `JobQueue` is a stdlib interface. Its default and only production implementation stores jobs in **the app's own database**, in a table the stdlib manages (`k_job`, outside `schema.lock`, like `k_session`). No Redis or broker is needed.
- **Enqueueing inside `database.transaction { … }` joins the transaction.** The job is stored only if the transaction commits, so "mark the order paid" and "send the confirmation" can't diverge: there is no paid order without its email, and no email for a rolled-back payment. This is the transactional-outbox pattern without writing an outbox.
- **The worker runs inside `dist/app serve`,** next to the HTTP server, claiming jobs with `SELECT … FOR UPDATE SKIP LOCKED` on Postgres. Several replicas share the queue safely. `K_JOB_CONCURRENCY` (default 4) limits concurrent jobs per process. On `SIGTERM`, the worker stops claiming jobs and waits for running ones within the shutdown grace period ([deployment.md](deployment.md)); unfinished jobs return to the queue.
- `k dev` runs the worker too, against the development database.

### Schedules

The cron schedules listed in `k.server` use the same mechanism: a job whose payload is `Void`, with `@Schedule(.every(1.hour))` or `@Schedule(.cron("0 3 * * *"))`. Each due run is enqueued once across all replicas. The shop doesn't use schedules; details are left to a later revision.

### Tests

- **`InlineJobQueue`** (from `k.test`) runs each job right after the enqueueing transaction commits, or at once outside a transaction, and lets retryable failures fail the test. With `use InlineJobQueue for JobQueue`, a test observes a job's effects (an email recorded by a fake `EmailProvider`) within the test.
- Without it, tests get **`RecordingJobQueue`**, the default in `k test`, which stores jobs without running them; `queue.enqueued(OrderConfirmationJob)` returns their payloads.
- Tests of a job call `run(payload:)` directly and check its `JobError`s with `expect throws`.

## Why

- Modelling a job as a service implementing an interface adds no syntax and reuses DI, fakes and layering.
- Storing jobs in the app's database makes transactional enqueueing possible and keeps deployment to one executable plus one database, which [13-tooling.md](../13-tooling.md) aims for.
- Typed `JobError` cases make the retry decision explicit at the point where the error is understood.

## Open questions

- A way to inspect and re-run dead jobs (a `dist/app jobs` command, or an admin page from the stdlib).
- Uniqueness keys (enqueue at most one pending job per order) and rate-limited queues.
- Whether `k build --target edge` can support jobs at all.
