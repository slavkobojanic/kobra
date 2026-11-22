# Data: persistence, migrations, loading, forms

Sources: `book/apps/persistence.md`, `async-data.md` and `forms.md`, and `design/14-async-data.md`,
`15-forms-and-validation.md` and `19-persistence.md`.

## Entities

```k
import k.crypto.Uuid
import k.db.Id
import k.db.Index
import k.db.Relation
import k.db.Version

enum OrderStatus implements Codable, Equatable {
    case awaitingPayment
    case paid
    case cancelled
}

/// A placed order.
public entity Order {
    @Id let id: Uuid
    @Relation let account: Account
    @Index var status: OrderStatus = .awaitingPayment
    var total: Money
    var note: String? = nil
    let placedAt: Date
    @Version var revision: Int = 1
}
```

- The annotations are `@Id`, `@Unique` (insert and update then throw `Entity.UniqueViolation`, with
  one case per unique field), `@Index`, `@Relation` and `@Version` (an optimistic lock), all imported
  from `k.db`.
- Tables and columns get snake-case names (`order`, `placed_at`), and a to-one relation is stored as
  `account_id`. `schema.lock` records the names, so renames never move data (record a rename with
  `k migrate rename shop.Order.note memo`).
- A to-many relation is `@Relation var lines: List<OrderLine> = []`, the other side of a to-one.
- Entities are server-only and constructed like structs: `Order(id: Uuid.random(), account: account,
  total: 48.00.usd, placedAt: clock.now)`.
- Postgres runs in production (`DATABASE_URL`), and SQLite in development (`.k/<package>.db`) and in
  tests (a fresh in-memory database per test).

## Repositories: queries are collection code

```k
import k.crypto.Uuid

/// Queries and status changes for `Order`.
public repository OrderRepository for Order {
    func byId(_ id: Uuid) async -> Order? {
        return await orders
            .including { order => order.account }
            .first { order => order.id == id }
    }

    func recent(for account: AccountId, limit: Int) async -> List<Order> {
        return await orders
            .filter { order => order.account.id == account and order.status != .cancelled }
            .sortedByDescending { order => order.placedAt }
            .limit(limit)
    }

    /// Moves an awaiting order to paid; `false` when it wasn't awaiting payment.
    func markPaid(id: Uuid) async -> Bool {
        let updated = await orders
            .filter { order => order.id == id and order.status == .awaitingPayment }
            .update { order => order.status = .paid }

        return updated == 1
    }
}
```

- Inside `repository R for Order`, `orders` (the lower-camel plural) is the table. The query calls
  are `filter`, `first`, `single`, `sortedBy`, `sortedByDescending`, `thenBy`, `limit`, `including`,
  and the bulk `.update { … }` and `.delete()`.
- **A query runs only when awaited, and every query is awaited** (`return await …`). A lambda that
  can't become SQL is a compile error. There are no method-name-derived queries.
- **Relations load only with `including`.** Reading one that wasn't loaded is a compile error or a
  panic, so N+1 queries can't happen by accident.
- The inherited writes are `insert(e)`, `insert(all:)`, `update(e)` (which returns `Bool` with a
  `@Version`) and `delete(e)`. They never write relations. Handle a duplicate on a unique field like
  this:

```k
do {
    try await accounts.insert(account)
} catch .email {
    throw SignUpError.emailTaken
}
```

## Transactions

```k
let recorded = await database.transaction {
    let marked = await orders.markPaid(id: orderId)

    if marked {
        await jobs.enqueue(OrderConfirmationJob.kind, payload: OrderConfirmation(orderId: orderId))
    }

    marked
}
```

`@Inject let database: Database` (`k.db`) gives you transactions. The block's last expression is its
value. The call throws what the block throws, and a nested transaction is a savepoint. Enqueued jobs and
`Broadcast` messages fire on the outermost commit. Use `database.ping(timeout:)` for health checks.

## Migrations

Run `k migrate new "create accounts and orders"` after you change entities. It writes
`migrations/NNNN_….k` and updates `schema.lock`. Review both and commit them. The output looks like
this:

```k
migration CreateAccountsAndOrders {
    func up(schema: Schema) {
        schema.createTable(name: "account") { table =>
            table.column(name: "id", type: .uuid).primaryKey(true)
            table.column(name: "email", type: .text).unique(true)
            table.column(name: "password_hash", type: .text)
            table.column(name: "created_at", type: .timestamp)
        }

        // …
    }
}
```

- `k migrate new` refuses unsafe changes: a required column without a default, a type change, an
  enum case change, or a removed entity. Write those by hand with `k migrate new --empty "…"`.
  Destructive steps need `@Breaking("reason")` on the migration.
- Removing a field *retires* its column. `k migrate new --drop-retired "…"` drops it in a later
  release.
- Migrations can't import anything, and they name tables and columns as strings. There is no `down`.
  `k migrate status` and `k migrate apply` work on the development database, and production applies
  migrations at startup.
- An entity without its migration fails `k check` (K8004).

## Loading data on a page

```k
/// The guestbook: every entry, and a form to add one.
public page Guestbook at "/guestbook" {
    @Inject let guestbook: GuestbookService

    @Load let entries: List<GuestbookEntry> = try await guestbook.entries()

    @Form var draft: SignRequest = .empty
    @State var problem: String? = nil

    func sign() async throws ValidationError {
        do {
            try await guestbook.sign(request: draft.value)

            draft.reset()
            reload()
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure {
            problem = "We couldn't save your entry. Try again."
        }
    }

    metadata {
        title("Guestbook")
    }

    view {
        main {
            heading(level: 1) { "Guestbook" }

            if entries.isEmpty {
                paragraph { "Nobody has signed yet. Be the first!" }
                    .class(.muted)
            } else {
                list {
                    for entry in entries key entry.id {
                        item {
                            EntryCard(entry: entry)
                        }
                    }
                }
            }

            form {
                textField(value: bind draft.name)
                    .label("Name")

                textArea(value: bind draft.message)
                    .label("Message")

                if let problem {
                    paragraph { problem }
                        .role(.alert)
                }

                button { "Sign the guestbook" }
                    .type(.submit)
                    .disabled(draft.isSubmitting)
            }
            .on(submit: draft.handle(sign))
        }
        .class(.page)
    }

    loading {
        paragraph { "Loading entries…" }
            .class(.muted)
    }

    failed {
        paragraph { "The guestbook couldn't be loaded." }

        button { "Try again" }
            .on(click: reload)
    }

    style {
        .page {
            maxWidth: 40rem;
            margin: 0 auto;
            padding: .lg;
        }

        .muted { color: .muted; }
    }
}
```

`@Load` rules (typed `failed(let error)`, `reload()`, `keepOnReload`, and `Codable` for server
rendering) are in the kobra-language skill's `components-and-views.md`.

## Forms and validation

```k
import k.validation.Length
import k.validation.NotBlank
import k.validation.Validatable

/// What a visitor fills in to sign the guestbook.
public struct SignRequest implements Codable, Validatable {
    @NotBlank @Length(max: 40) var name: String
    @NotBlank @Length(max: 500) var message: String
}
```

- The constraints are `@NotBlank`, `@Length(min:max:)`, `@Range(13...120)`, `@Matches("regex
  literal")`, `@Email` (only on a `String` field), `@Format(message:)` (on parsed types) and
  `@Check(Service.function)` (async, server-backed). Each takes `message:`, and all are imported from
  `k.validation`.
- A typed field validates itself: `var email: EmailAddress` binds to a `textField` and parses.
- Cross-field rules go in `func validate(rules: Rules) { rules.check(a == b, field: .b, message: "…") }`.
- `@Form var draft: T = .empty`, or prefill with `.empty(field: value, …)`. Its members are
  `isSubmitting`, `isValid`, `errors`, `isTouched`, `reset()` and `handle(action)`.
- **`draft.value` is readable only inside the action passed to `handle`** (that is, after validation).
  The action either doesn't throw or throws only `ValidationError`. Rethrow
  `.rpcFailure(.invalid(let failure))` to show server-side failures under their fields.
- The same struct is the rpc parameter, and the server validates it again before the handler runs.
- A failed submit focuses the first invalid field. Errors and ARIA wiring are automatic, so give each
  field a `.label("…")`.
