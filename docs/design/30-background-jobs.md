# Background Jobs

## Decision

**A job is a service that implements `Job<Payload>`.** Work is enqueued through the injected **`JobQueue`**, stored in the app's own database in the enqueuing transaction, and run by a worker inside `dist/app serve`.

```k
@Retry(attempts: 8, backoff: 30.seconds)
@JobName("order-confirmation")
service OrderConfirmationJob implements Job<OrderConfirmation> {
    @Inject let orders: OrderRepository
    @Inject let mailer: EmailProvider
    @Inject let urls: PublicUrls

    func run(payload: OrderConfirmation) async throws JobError {
        guard let order = await orders.byId(id: payload.orderId) else {
            throw JobError.permanent(reason: "Order {payload.orderId} no longer exists")
        }

        do {
            try await mailer.send(email: confirmation(order: order))
        } catch .unavailable {
            throw JobError.retryable(reason: "The mail server is unavailable")
        } catch let error {
            throw JobError.permanent(reason: "The message was refused: {error}")
        }
    }
}

struct OrderConfirmation implements Codable {
    let orderId: Uuid
}

service PaymentRecorder {
    @Inject let jobs: JobQueue
    @Inject let database: Database
    @Inject let orders: OrderRepository

    func recordPayment(orderId: Uuid, reference: String) async -> Bool {
        return await database.transaction {
            let marked = await orders.markPaid(id: orderId, reference: reference)

            if marked {
                await jobs.enqueue(OrderConfirmationJob.kind, payload: OrderConfirmation(orderId: orderId))
            }

            return marked
        }
    }
}
```

## Jobs

```k
public interface Job<Payload> where Payload implements Codable {
    func run(payload: Payload) async throws JobError
}

public enum JobError implements Error {
    case retryable(reason: String)    // try again after the backoff
    case permanent(reason: String)    // never try again
}
```

- **No new keyword.** A job is a service with an interface, so injection and fakes work as for any service. `Job` is server-only, so a job is server-only too.
- **Each job has a generated `static let kind: JobKind<Payload>`,** the typed handle used to enqueue it and to inspect it in tests. A type is never passed as a value.
- **`@JobName("order-confirmation")`** gives the job a stable name for its rows in the queue. Without it, the name is derived from the service's name when first recorded. Either way it is recorded in `storage.lock` with the payload's `Codable` shape, so renaming the service doesn't strand queued jobs ([Stored payloads](#stored-payloads)).
- **The payload is `Codable`,** because it is stored. Prefer ids over copies of data: the job then reads current data when it runs, and no personal data sits in the queue. A payload that contains a `Secret` is a compile error ([25-config-and-secrets.md](25-config-and-secrets.md)). `Void` is `Codable` as `{}`.
- **`run` is called only by the worker and by test code.** Calling it anywhere else is a compile error, so the queue, its retries and its transactional guarantee can't be bypassed.
- **Each run gets a fresh container,** which works like a request's ([10-services.md](10-services.md#scopes)). A job has no session: if its dependency graph reaches `Session` or `SessionManager`, that is a compile error naming the path. A job that needs a user's id carries it in its payload.

## Enqueueing

- **`await jobs.enqueue(OrderConfirmationJob.kind, payload: …, delay: 0.seconds)`** on the injected `JobQueue`. The payload's type must match the job's. `delay` defaults to none.
- `enqueue` doesn't throw. Failing to store a job is an infrastructure failure, which panics like other database failures ([19-persistence.md](19-persistence.md#writes)).
- **Inside a transaction, the job is stored in that transaction** and becomes visible to the worker when the app-level transaction commits. It is discarded on rollback, so "mark the order paid" and "send the confirmation" can't diverge: there is no paid order without its email, and no email for a rolled-back payment. This is the transactional-outbox pattern without writing an outbox.

## Retries and dead jobs

- **`@Retry(attempts:backoff:)`** on the job sets the policy: up to `attempts` runs in total, waiting `backoff`, then doubling each time, with jitter. Without it, the default is 5 attempts from 10 seconds.
- `JobError.retryable` and a panic during `run` count as failed attempts. `JobError.permanent` stops at once.
- A job that runs out of attempts, or fails permanently, is kept as **dead** with its last reason, and logged at error level through `k.observe`. Dead jobs are never retried automatically.
- **Dead jobs are visible:** `dist/app jobs list --dead` lists them and `dist/app jobs retry <id>` re-queues one ([38-deployment.md](38-deployment.md#commands)). `k.observe` exposes a dead-job count as a metric for alerting.
- **Delivery is at least once.** If the process dies after the work but before the job is acknowledged, it runs again, so a job must be idempotent or accept rare duplicates.

## The queue and the worker

- `JobQueue` is a server-only stdlib platform interface ([10-services.md](10-services.md#layering)). Its production implementation stores jobs in **the app's own database**, in a table the stdlib manages (`k_job`, outside `schema.lock`). No Redis or broker is needed.
- **The worker runs inside `dist/app serve`,** next to the HTTP server, claiming jobs with `SELECT … FOR UPDATE SKIP LOCKED` on Postgres. Several replicas share the queue safely.
- **Concurrency** is `JobConfig.concurrency` (`K_JOB_CONCURRENCY`, default 4, between 1 and 256) jobs per process ([25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)).
- **On `SIGTERM`,** the worker stops claiming jobs and waits for running ones within `ServerConfig.shutdownGrace`; unfinished jobs return to the queue ([38-deployment.md](38-deployment.md#startup-and-shutdown)).
- `k dev` runs the worker too, against the development database.

## Schedules

A job whose payload is `Void` can run on a schedule:

```k
@Schedule(.daily(at: TimeOfDay("03:00")))
service PurgeExpiredCarts implements Job<Void> { … }
```

- **Schedules are typed:** `.every(1.hour)`, `.daily(at: TimeOfDay)`, `.weekly(on: .monday, at: TimeOfDay)`. Times are UTC. There is no cron string.
- Each due run is enqueued once across all replicas, and then runs like any other job, with its retries.

## Stored payloads

Jobs queued during a rolling deploy are run by the next release, so the queue's format is recorded in **`storage.lock`** with the rules of `protocol.lock` ([11-protocols-and-rpc.md](11-protocols-and-rpc.md#protocollock)):

- Each job's name and payload shape are recorded.
- Adding an optional field to a payload is compatible. Removing or renaming a field, changing its type, or adding a required field is a `k check` error unless the job is marked `@Breaking("reason")`.

## Tests

- **`k test` installs `RecordingJobQueue` by default,** which stores jobs without running them ([21-testing.md](21-testing.md#test-doubles-and-defaults)). `queue.payloads(of: OrderConfirmationJob.kind)` returns what was enqueued, in order.
- **`use InlineJobQueue for JobQueue`** runs each job when the enqueuing app-level transaction commits, or at once outside a transaction, and fails the test on a retryable failure. A test then observes a job's effects (an email in `RecordingEmailProvider.sent`).
- Tests of a job call `run(payload:)` directly and check its `JobError`s with `expect throws`.

## Why

- Modelling a job as a service implementing an interface adds no syntax and reuses DI, fakes and layering.
- Enqueueing through the injected queue makes the wiring visible DI, and `kind` keeps it typed without passing types as values.
- Storing jobs in the app's database makes transactional enqueueing possible and keeps deployment to one executable plus one database.
- Typed `JobError` cases make the retry decision explicit at the point where the error is understood.

## Open questions

- Uniqueness keys (enqueue at most one pending job per order) and rate-limited queues.
- Whether `k build --target edge` can support jobs at all.
