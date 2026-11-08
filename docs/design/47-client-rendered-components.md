# Client-Rendered Components

> **Status: proposal.** Not adopted. Nothing here is implemented, and the diagnostic codes are tentative.

## Decision

**`@Rendering(.client)` on a component** renders that component in the browser only, inside a page that is otherwise rendered on the server. The server renders the component's **`loading` block** in its place, and the browser swaps in its `view` right after hydration. Everything outside the component keeps its server-rendered first paint.

```k
import todo.TodoService

/// The todo list. Todos live in the browser's local storage, which the
/// server can't read, so only this component renders on the client; the
/// page around it (heading, filters, footer) is server-rendered.
@Rendering(.client)
component TodoList {
    @Prop let filter: Filter

    @Inject let todos: TodoService

    view {
        list {
            for todo in todos.visible(filter: filter) {
                TodoRow(todo: todo)
            }
        }
    }

    loading {
        TodoListSkeleton(rows: 3)
    }
}
```

Today the only way to render this list correctly is `@Rendering(.client)` on the whole app ([23-app-structure.md](23-app-structure.md#rendering-mode)). The app then loses server rendering for every page. The other choice is the `K4013` warning, because the first paint shows the service's empty initial state and then jumps. With this proposal, the client-side part is exactly the part that needs the browser.

This is only the **client-only subtree** half of mixed rendering. The other half, server-only components inside client-rendered code in the style of React Server Components, is out of scope ([why](#why-not-server-only-components)).

## Rules

- **The annotation is allowed on components only,** with `.client`. `@Rendering(.server)` on a component is an error, since the server is the default and a component can't opt back in inside a client-rendered parent (tentatively `K4027`). Per-page `@Rendering` stays an open question ([23-app-structure.md](23-app-structure.md#open-questions)).
- **`loading` is required,** as it is for `@Load` ([14-async-data.md](14-async-data.md#rules)). It is what the server renders and what the browser hydrates. `loading { }` renders nothing, which is the explicit choice to reserve no space.
- **`loading` may read only the component's `@Prop`s,** plus constants and pure stdlib calls. On the server, the component isn't constructed: no field initializer, `@Derived`, `@Load`, `@Inject` or effect is evaluated. Reading any other member from `loading` is an error at the read, with a label at the member (tentatively `K4028`).
- **Everything except `loading` runs only in the browser.** So code in the component's `view`, `@Derived` and `@Load` initializers, and field initializers can call client-only interfaces. `K4012` doesn't apply there, and reading a client-only service's state isn't `K4013`.
- **The subtree is client-rendered transitively.** Every component the `view` renders, and every slot it renders, renders in the browser. Their `@Load`s run in the browser, so their values don't need to be `Codable` because of this component: nothing is serialized for them. They still need to be `Codable` if the same component is also used in a server-rendered place.
- **Props are ordinary props.** The whole page hydrates, so the parent passes props from its own hydrated state, as it does for any child. Props don't need to be `Codable`, and handlers and bindings work as usual. This is the main difference from server components.
- **In an app that already has `@Rendering(.client)`,** the annotation has no effect. That is a warning with a fix that removes it (tentatively `K4029`, machine-applicable).

### Diagnostics that change

- **`K4013` gets a second fix:** "Add `@Rendering(.client)` to the component", offered first when the read happens in a component rather than a page. The fix also inserts `loading { }` when the component has none. "Add `@Rendering(.client)` to the app" is still offered.
- **A component with `@Rendering(.client)` and no `loading`** reuses the missing-`loading` error of `@Load`, with a fix that inserts `loading { }`.

## Rendering and hydration

A client-rendered component works like **a component with one more load that resolves only in the browser, right after hydration**. This reuses the machinery of `@Load`, and it keeps the server's markup and the hydration pass identical without any new bookkeeping.

1. **On the server,** the component's slot renders `loading` from its props, framed as a block range `<!--[-->…<!--]-->`, the same framing a `@Load` branch uses ([runtime.md §6](../compiler/runtime.md#6-hydration-protocol)). It writes nothing to `k-data`.
2. **During hydration,** the browser also renders `loading`, and claims the server's nodes like any other branch. Both sides render the same nodes in the same order, so the `uid()` counters and instance keys (`0.2.1`) of everything after the component match without any adjustment. Ids are counted per pass from `k1` on both sides, and the component's `view`, which only the browser renders, allocates its ids after the pass.
3. **After the pass,** the branch switches to `view`. The switch is queued after the `init()` of every client-only service the pass created (`afterHydration`), so the first `view` sees the state those `init()`s loaded, for example from local storage. If the component also has `@Load`s, they start at that point, and `loading` stays until they resolve, as on any first load.
4. **On client navigation,** nothing is server-rendered, so the component renders `view` at once (or `loading` while its own `@Load`s run), like any other component.

### What the user sees

| Moment | Server-rendered parts | Client-rendered component |
|---|---|---|
| First paint (HTML) | final content | `loading` |
| Hydration pass | claimed, interactive | `loading`, claimed |
| Right after hydration | unchanged | `view` |
| Client navigation | rendered in the browser | `view` (or `loading` while its loads run) |

A `loading` block the same size as the `view` (a skeleton) avoids layout shift. The compiler can't check that, so it is guidance for authors, not a rule.

## Interactions

- **A server-rendered sibling that reads the same client-only service** still shows the initial state on first paint, and still gets `K4013`. The fix now offers to make *that* component client-rendered too.
- **`metadata`:** a client-rendered component can't contain a `metadata` block, because the server renders the document head before anything in the browser runs. Metadata already belongs only to pages, layouts and the app ([22-metadata.md](22-metadata.md#rules)), so no new rule is needed.
- **Error boundaries:** an error thrown while rendering `view` in the browser goes to the nearest `ErrorBoundary`, as for any browser-side render.
- **Tests:** `render(…)` mounts in the browser container ([21-testing.md](21-testing.md)), so a test sees `view`, after a flush. A server request through `TestClient` sees `loading`. That's the behavior to assert, for example that the skeleton is present in the HTML.
- **Tooling:** hover over the component's name says "rendered in the browser only; the server renders `loading`". `k routes` doesn't change.
- **Bundles:** the server bundle needs only the component's `loading` renderer, so it can drop its `view` code. The client bundle doesn't change: the component already hydrates.

## Why

- **It reuses `loading`,** an existing idea in K: a declared view for "not ready yet". A client-rendered component is a component whose data isn't ready until the browser runs. Authors already write `loading` for `@Load`, and it already means the right thing.
- **It makes the hydration problems go away instead of solving them.** Rendering `loading` on both sides during the pass means ids, instance keys and claimed nodes line up without reserving id ranges or skipping paths. The swap happens after the pass, when nothing is being claimed any more.
- **The boundary is per declaration,** like the server/client boundary of services ([10-services.md](10-services.md#serverclient-boundary)). There's no inline `clientOnly { }` view block: an annotation on a component names the boundary and keeps it in one place, and moving a part of a view into a component to annotate it is a small refactor.
- **It is a finer version of a choice the language already offers** (`@Rendering(.client)` on the app) and of a warning it already gives (`K4013`), not a new concept.
- **`loading` reading only props** means the server never constructs the component. So no field initializer or injected client-only service has to work on the server, and there's no rule about which members are safe there.

### Why not server-only components

Server-only components would render with server-only data and never ship JS, in the style of React Server Components. They are a much larger feature:

- After the first load, pages render in the browser, so a server component would need an rpc that returns rendered HTML or a serialized tree on every navigation and every input change.
- Every prop crossing the boundary would have to be `Codable`, and handlers and bindings couldn't cross at all.
- Client components inside a server component would need their own placeholder and hydration protocol.
- It would split components into two kinds with different rules, while K draws the server/client boundary per declaration through services, protocols and `@Load`. Those already give server-only data to any component without shipping the server code.

None of the examples needs this. It can be reconsidered if bundle size from large, static, data-heavy views becomes a measured problem.

## Open questions

- **Per-page `@Rendering(.client)`.** The same mechanism would work: the server renders the layouts and the page's `loading`. But page `metadata` usually reads page state, which the server would no longer have. It stays open in [23-app-structure.md](23-app-structure.md#open-questions).
- **Slots in `loading`.** Should `loading` be able to render a slot passed by a server-rendered parent? Allowing it would put parent content into the server's HTML, and the swap would then need to move that content into `view` instead of rendering it again.
- **A lazy variant**, `@Rendering(.client(when: .visible))`, which would swap when the component scrolls into view instead of right after hydration. It would be useful for heavy widgets below the fold, but nothing needs it yet.
- **Whether the swap should wait for `@Load`s of the subtree** before replacing `loading`, to avoid a skeleton-to-skeleton flash. Today each component with loads is its own loading boundary ([14-async-data.md](14-async-data.md#rules)), and this proposal keeps that.
