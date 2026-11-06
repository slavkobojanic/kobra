# Background jobs

**A job is a service that implements `Job<Payload>`.** Work is enqueued through the injected
**`JobQueue`**, stored in the app's own database in the enqueuing transaction, and run by a worker inside
`dist/app serve`. No Redis or message broker is needed.

```k
/// Emails the customer once their payment has been recorded.
@Retry(attempts: 8, backoff: 30.seconds)
@JobName("order-confirmation")
public service OrderConfirmationJob implements Job<OrderConfirmation> {
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

    private func confirmation(order: Order) -> Email {
        let receipt = urls.absolute(.receipt(id: order.id))

        return Email(to: order.email, subject: "Your order is confirmed", text: "View your receipt: {receipt}")
    }
}

struct OrderConfirmation implements Codable {
    let orderId: Uuid
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
- **`run` is called only by the worker and by test code.** Calling it anywhere else is a compile error,
  so the queue, its retries and its transactional guarantee can't be bypassed.
- **Each run gets a fresh container,** which works like a request's. A job has no session: if its
  dependency graph reaches `Session` or `SessionManager`, that is a compile error naming the path. A job
  that needs a user's id carries it in its payload.

## Enqueueing

```k
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

            marked
        }
    }
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
  count is exposed as a metric for alerting.
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
@Schedule(.daily(at: TimeOfDay("03:00")))
service PurgeExpiredCarts implements Job<Void> { … }
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
  test PaymentRecorder {
      @Inject let recorder: PaymentRecorder
      @Inject let queue: RecordingJobQueue

      test "a recorded payment enqueues one confirmation" {
          let marked = await recorder.recordPayment(orderId: Order.awaitingPayment.id, reference: "pi_123")

          expect marked
          expect queue.payloads(of: OrderConfirmationJob.kind).count == 1
      }
  }
  ```

- **`use InlineJobQueue for JobQueue`** runs each job when the enqueuing transaction commits, or at once
  outside a transaction, and fails the test on a retryable failure. A test then observes a job's effects,
  such as an email in `RecordingEmailProvider.sent`.
- Tests of a job itself call `run(payload:)` directly and check its `JobError`s with `expect throws`.

> **Open question.** Uniqueness keys (at most one pending job per order) and rate-limited queues aren't
> designed yet.
