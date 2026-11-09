# Background jobs

**A job is a service that implements `Job<Payload>`.** Work is enqueued through the injected
**`JobQueue`**, stored in the app's own database in the enqueuing transaction, and run by a worker inside
`dist/app serve`. No Redis or message broker is needed.

```k
// examples/shop/shop/src/store/OrderConfirmationJob.k
@Retry(attempts: 8, backoff: 30.seconds)
@JobName("order-confirmation")
service OrderConfirmationJob implements Job<OrderConfirmation> {
    @Inject let orders: OrderRepository
    @Inject let mailer: EmailProvider
    @Inject let urls: PublicUrls

    func run(payload: OrderConfirmation) async throws JobError {
        guard let order = await orders.byId(payload.orderId) else {
            throw JobError.permanent(reason: "Order {payload.orderId} no longer exists")
        }

        let email = Email(
            to: order.email,
            subject: "Your K Shop order is confirmed",
            text: confirmationText(for: order)
        )

        do {
            try await mailer.send(email: email)
        } catch .unavailable {
            throw JobError.retryable(reason: "The SMTP server is unavailable")
        } catch .rejected(let reason) {
            throw JobError.permanent(reason: "The SMTP server rejected the message: {reason}")
        } catch .invalidMessage(let reason) {
            throw JobError.permanent(reason: "The message can't be sent as written: {reason}")
        }
    }
    // ...
}
// ...
struct OrderConfirmation implements Codable, Equatable {
    let orderId: Uuid
}
```

## Jobs

```k
// crates/k_sema/stdlib/k/server/Job.k
public interface Job<Payload> where Payload implements Codable {
```

```k
// crates/k_sema/stdlib/k/server/JobError.k
public enum JobError implements Error {
    /// The run failed for now: try again after the backoff ([`Retry`]).
    case retryable(reason: String)
    /// The run can never succeed: don't try again; the job becomes dead.
    case permanent(reason: String)
}
```

`Job` has one requirement, `func run(payload: Payload) async throws JobError`.

- **No new keyword.** A job is a service with an interface, so injection and fakes work as for any
  service. `Job` is server-only, so a job is server-only too.
- **Each job has a generated `static let kind: JobKind<Payload>`,** the typed handle used to enqueue it and
  to inspect it in tests. A type is never passed as a value.
- **`@JobName("order-confirmation")`** gives the job a stable name for its rows in the queue. Without it,
  the name is derived from the service's name when first recorded. Either way it is recorded in
  `storage.lock` with the payload's shape, so renaming the service doesn't strand queued jobs.
- **The payload is `Codable`,** because it is stored. Prefer ids over copies of data: the job then reads
  current data when it runs, and no personal data sits in the queue. A payload containing a `Secret` is a
  compile error. `Void` is `Codable` as `{}`.
- **`run` is called only by the worker and by test code,** so the queue, its retries and its
  transactional guarantee aren't bypassed. The compiler doesn't reject another call yet
  (**Planned** job-run-check).
- **Each run gets a fresh container,** which works like a request's. A job has no session: if its
  dependency graph reaches `Session` or `SessionManager`, that is a compile error naming the path. A job
  that needs a user's id carries it in its payload.

## Enqueueing

```k
// examples/shop/shop/src/store/PaymentRecorder.k
public service PaymentRecorder {
    @Inject let orders: OrderRepository
    @Inject let database: Database
    @Inject let jobs: JobQueue
    @Inject let clock: Clock
    // ...
    let recorded = await database.transaction {
        let marked = await orders.markPaid(id: order.id, reference: reference, paidAt: clock.now)

        if marked {
            await jobs.enqueue(OrderConfirmationJob.kind, payload: OrderConfirmation(orderId: order.id))
        }

        marked
    }
    // ...
}
```

- **`await jobs.enqueue(Job.kind, payload: …, delay: …)`** on the injected `JobQueue`. The payload's type
  must match the job's, and `delay` defaults to none.
- `enqueue` doesn't throw. Failing to store a job is an infrastructure failure, which panics like other
  database failures.
- **Inside a transaction, the job is stored in that transaction** and becomes visible to the worker when
  the outermost transaction commits. It is discarded on rollback, so there is no paid order without its
  email, and no email for a rolled-back payment. This is the transactional-outbox pattern without writing
  an outbox.

## Retries and dead jobs

- **`@Retry(attempts:backoff:)`** sets the policy: up to `attempts` runs in total, waiting `backoff`,
  then doubling each time, with jitter. Without it, the default is 5 attempts from 10 seconds.
- `JobError.retryable` and a panic during `run` count as failed attempts. `JobError.permanent` stops at
  once.
- A job that runs out of attempts, or fails permanently, is kept as **dead** with its last reason, and
  logged at error level. Dead jobs are never retried automatically.
- `dist/app jobs list --dead` lists dead jobs and `dist/app jobs retry <id>` re-queues one. A dead-job
  count as a metric for alerting comes with metrics in `k.observe` (**Planned** observe-metrics).
- **Delivery is at least once.** If the process dies after the work but before the job is acknowledged,
  it runs again, so a job must be idempotent or accept rare duplicates.

## The queue and the worker

- The production `JobQueue` stores jobs in **the app's own database**, in a standard table (`k_job`,
  outside `schema.lock`).
- **The worker runs inside `dist/app serve`,** next to the HTTP server, claiming jobs with
  `SELECT … FOR UPDATE SKIP LOCKED` on Postgres, so several replicas share the queue safely. `k dev` runs
  the worker too.
- **Concurrency** is `K_JOB_CONCURRENCY` jobs per process, 4 by default.
- **On `SIGTERM`,** the worker stops claiming jobs and waits for running ones, within `K_SHUTDOWN_GRACE`;
  unfinished jobs return to the queue.

## Schedules

A job whose payload is `Void` can run on a schedule:

```k
~import k.server.Schedule
~import k.time.TimeOfDay
~import k.server.Job
~import k.server.JobError
~
@Schedule(.daily(at: TimeOfDay("03:00")))
service PurgeExpiredCarts implements Job<Void> {
    func run(payload: Void) async throws JobError {
        // …
    }
}
```

- **Schedules are typed:** `.every(1.hour)`, `.daily(at: TimeOfDay)`, `.weekly(on: .monday, at:
  TimeOfDay)`. Times are UTC. There is no cron string.
- Each due run is enqueued once across all replicas, and then runs like any other job, with its retries.

## Stored payloads

Jobs queued during a rolling deploy are run by the next release, so each job's name and payload shape are
recorded in **`storage.lock`**, in the format described in
[Local storage](local-storage.md#file-format). Adding an optional field to a payload is compatible. Removing or renaming
a field, changing its type, or adding a required field is a `k check` error unless the job is marked
`@Breaking("reason")`.

## Tests

- **`k test` installs `RecordingJobQueue` by default,** which stores jobs without running them.
  `queue.payloads(of: OrderConfirmationJob.kind)` returns what was enqueued, in order:

  ```k
  // examples/shop/shop/src/store/PaymentRecorder.test.k
  test PaymentRecorderQueue {
      @Inject let recorder: PaymentRecorder
      @Inject let catalog: ProductRepository
      @Inject let orders: OrderRepository
      @Inject let queue: RecordingJobQueue
      @Inject let clock: TestClock

      test "a recorded payment enqueues one confirmation for its order" {
          let mug = Product.mug()
          let order = Order.sample(product: mug, placedAt: clock.now)
          try await catalog.insert(mug)
          await orders.insert(order)

          await recorder.recordPayment(orderId: order.id, amount: 48.00.usd, reference: "cs_test_1")
          await recorder.recordPayment(orderId: order.id, amount: 48.00.usd, reference: "cs_test_1")

          expect queue.payloads(of: OrderConfirmationJob.kind) == [OrderConfirmation(orderId: order.id)]
      }
  }
  ```

- **`use InlineJobQueue for JobQueue`** runs each job when the enqueuing transaction commits, or at once
  outside a transaction, and fails the test on a retryable failure. A test then observes a job's effects,
  such as an email in `RecordingEmailProvider.sent`.
- Tests of a job itself call `run(payload:)` directly and check its `JobError`s with `expect throws`.

> **Open question.** Uniqueness keys (at most one pending job per order) and rate-limited queues aren't
> designed yet.
