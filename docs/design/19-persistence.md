# Persistence

## Decision

Database access uses **entities and repositories** (Spring Data's structure), with **LINQ-style queries**: ordinary collection lambdas that the compiler translates to SQL.

```k
entity User {
    @Id let id: UserId
    @Unique var email: String
    var name: String
    var lastSeen: Date
    @Relation var orders: List<Order>
}

repository UserRepository for User {
    func byEmail(email: String) async -> User? {
        return users.first { user => user.email == email }
    }

    func activeSince(date: Date) async -> List<User> {
        return users
            .filter { user => user.lastSeen > date }
            .sortedBy { user => user.name }
            .limit(50)
    }
}

service AccountService {
    @Inject let users: UserRepository
}
```

## Rules

- **`entity` and `repository` are declaration keywords. Both are server-only.** Only server-side services may inject repositories (see [10-services.md](10-services.md)).
- **Queries use the standard collection API.** The compiler translates the lambdas to SQL at compile time. A query that can't be translated (for example, calling an arbitrary function inside `filter`) is a **compile error**, never a silent in-memory scan.
- **There are no queries derived from method names** like `findByEmailAndStatusOrderByCreatedAtDesc`. Lambdas are the one way to write queries.
- **Relations load only when requested:** `.including { user => user.orders }`. Reading an unloaded relation is a compile error where the compiler can tell, and a panic otherwise. This prevents N+1 queries by default.
- **Transactions:** `try await db.transaction { ... }` rolls back on a throw.
- **Databases:** Postgres in production; SQLite for local development and tests.

## Migrations: `schema.lock`

- `schema.lock` records the current database schema.
- `k migrate new` generates a reviewable migration from the difference between the entities and the lock.
- Destructive changes (dropping a column, narrowing a type) require `@Breaking("reason")`.
