---
name: kobra-apps
description: Building full-stack web apps in Kobra (.k files, App.k, k.toml). Use when creating or changing a Kobra app's pages, layouts, routing, services and dependency injection, protocols/RPC, entities, repositories and migrations, @Load data, forms and validation, sign-in, sessions and guards, configs and secrets, background jobs, email, HTTP clients, endpoint pages and webhooks, server streaming and channels, local storage, workspaces of several services, or deployment.
---

# Building apps in Kobra

A Kobra app is one project (a `k.toml`, an `App.k` and `src/`) that compiles into a server
executable and a browser bundle. The compiler draws the server/client boundary per declaration, wires
dependency injection at compile time, generates routes and the RPC client, and checks lockfiles for
anything external systems depend on.

**Validate as you go.** Write each file, run `check_snippet` on it (kobra MCP server), and fix every
diagnostic. Then run `k check` in the project, which also runs the tests and checks the lockfiles. When
you need an API this skill doesn't show, `search_docs` it (`IncomingRequest`, `@Schedule`, `Broadcast`)
and read the book page. The example apps (`list_examples`, `read_example`, such as `notes`, `shop` and
`huddle`) are complete, tested models. For language syntax, see the **kobra-language** skill. For
commands and tests, see **kobra-tooling**.

## Project layout

```text
k.toml           [package] name, k = "0.1" (the toolchain), title, description, version
App.k            the one `app` declaration
src/<module>/    every directory is a module; one public declaration per file, named after it
src/testing/     test-only fakes and fixtures
migrations/      NNNN_description.k, written by `k migrate new`
*.lock           routes.lock, schema.lock, rpc.lock, storage.lock, k.lock: generated and committed
.env             local development config (git-ignored)
```

`k new my-app` creates a project that already passes `k check`. Nothing is registered by hand:
pages, services, the theme, configs, jobs and protocol implementations are found from their
declarations.

## The layers

| Kind | Runs on | May inject |
|---|---|---|
| `component`, `page`, `layout` | browser (and server rendering) | **services only** |
| `service` | both, unless it injects something server-only or client-only | services, protocols, platform interfaces; server-side ones also repositories, configs, `Database`, `SessionManager` |
| `service X implements SomeProtocol` | server | the same (it serves the RPC) |
| `protocol` | the contract | nothing |
| `entity`, `repository` | server | repositories, configs |
| `config` | server | nothing (it is data) |
| endpoint `page` (HTTP handlers, no view) | server | any services |

**Components never call protocols directly** (K4005). A client `service` injects the protocol and
holds the caching, retries and shared state.

## End to end, in one module

```k
entity Entry {
    @Id let id: Uuid
    var name: String
    var message: String
    var createdAt: Date
}
```

```k
repository EntryRepository for Entry {
    func latest() async -> List<Entry> {
        return await entries
            .sortedByDescending { entry => entry.createdAt }
            .limit(100)
    }
}
```

```k
import k.auth.RateLimit
import k.rpc.RpcOnlyError

/// What the browser can ask the guestbook server.
public protocol GuestbookProtocol {
    rpc entries() throws RpcOnlyError -> List<GuestbookEntry>

    rpc sign(request: SignRequest) throws RpcOnlyError -> GuestbookEntry requires RateLimit(10.perMinute)
}
```

```k
import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.time.Clock

service GuestbookApi implements GuestbookProtocol {
    @Inject let repository: EntryRepository
    @Inject let clock: Clock

    rpc entries() async throws RpcOnlyError -> List<GuestbookEntry> {
        let latest = await repository.latest()

        return latest.map(summarize)
    }

    rpc sign(request: SignRequest) async throws RpcOnlyError -> GuestbookEntry {
        let entry = Entry(id: Uuid.random(), name: request.name, message: request.message, createdAt: clock.now)

        await repository.insert(entry)

        return summarize(entry: entry)
    }

    private func summarize(entry: Entry) -> GuestbookEntry {
        return GuestbookEntry(id: entry.id, name: entry.name, message: entry.message)
    }
}
```

```k
import k.rpc.RpcOnlyError

public service GuestbookService {
    @Inject let api: GuestbookProtocol

    func entries() async throws RpcOnlyError -> List<GuestbookEntry> {
        return try await api.entries()
    }

    func sign(request: SignRequest) async throws RpcOnlyError {
        try await api.sign(request: request)
    }
}
```

The page (`@Load`, `@Form`, `loading` and `failed`) and the `Codable` and `Validatable` structs are in
[references/data-and-forms.md](references/data-and-forms.md). After adding an entity, run
`k migrate new "create entries"`.

## Rules that matter most

- **Every rpc declares `throws`** with an error enum that has the case `rpcFailure(RpcFailure)`
  (K5501, K5502), or `throws RpcOnlyError`. The signature order is
  `rpc name(params) throws E -> T requires Guards`.
- **Guards go on the protocol (or a page), never on the handler** (K5507) or a layout (K5014). Guards
  are imported: `import k.auth.Authenticated`, `import k.auth.RateLimit`, `import k.auth.Role`.
- Inside a handler of an `Authenticated` rpc, or a page that `requires Authenticated`,
  `session.user` is a `SessionUser`, not optional.
- **The current time is `clock.now`** from `@Inject let clock: Clock` (`k.time`). Nothing else reads
  the time, so tests control it.
- **Entities never reach the browser.** Map them to `Codable` structs in the protocol's service.
- **Every query is awaited explicitly**, even when it's returned. A query lambda that can't become
  SQL is a compile error.
- **Pages are typed values.** Write `link(to: .orderDetail(id: id))` and
  `Router.navigate(to: .signIn.returnTo(.orders))`. Never write URL strings for your own pages.
- **Environment differences live in `App.k`** `use … for … in .production` rules and in `config`
  declarations. There's no `if production` in code.
- **Secrets are `Secret<T>`** in a `config`. Compare them with `.matches(…)` and reveal them only
  with `.reveal()`. They never reach the browser.
- `@Load` types, rpc parameters and returns, and job payloads are `Codable`.
- Lockfiles (`routes.lock`, `schema.lock`, `rpc.lock`, `storage.lock`) fail `k check` on breaking
  changes unless they're marked `@Breaking("reason")`. `k build` and `k migrate` write them, never by
  hand.

## Reference files

| File | Covers |
|---|---|
| [references/structure-and-routing.md](references/structure-and-routing.md) | `k.toml`, `App.k`, `use` rules and environments, pages, `@Param`, URLs, `routes.lock`, layouts, guards, metadata |
| [references/services-and-rpc.md](references/services-and-rpc.md) | services, injection, scopes, `init()`, layering, protocols, typed errors over the wire, streaming, `Broadcast`, channels |
| [references/data-and-forms.md](references/data-and-forms.md) | entities, repositories, queries, writes, `@Unique`, `@Version`, transactions, migrations, `@Load` pages, forms and validation |
| [references/auth-and-config.md](references/auth-and-config.md) | `Session`, `SessionManager`, `AuthUserId`, passwords, guards, `config`, `Secret`, `.env`, standard configs |
| [references/server-features.md](references/server-features.md) | jobs, email, `HttpClient`, endpoint pages and webhooks, health checks, local storage, timers, `PublicUrls` |
| [references/workspaces-and-deploy.md](references/workspaces-and-deploy.md) | workspaces, service-to-service RPC, `k build`, `dist/app`, containers, static hosting |
