# Proposal: Binding the Whole Error in `catch`

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`.

## Problem

[09-errors.md](../09-errors.md) requires errors to be **wrapped at layer boundaries** (`case network(RpcError)`), and shows `catch` clauses that match individual cases (`catch .outOfStock(let available)`). It doesn't show how to catch "every remaining case" while keeping the error value, which is exactly what wrapping needs. Listing each case (`catch .offline, .timeout, .status`) loses the value, and `match` has `default` but no binding form.

## Proposal

**`catch let name` binds the whole thrown value** and matches every case not handled by an earlier clause. It's the `catch` counterpart of a binding pattern in `match`.

```k
private func fetch<T>(url: Url, query: Map<String, String>) async throws WeatherError -> T where T implements Codable {
    do {
        return try await http.fetchJson(request: HttpRequest(method: .get, url: url).query(query))
    } catch .decoding(let message) {
        throw WeatherError.invalidResponse(reason: message)
    } catch let error {
        throw WeatherError.network(error)            // error: HttpError
    }
}
```

- `name` has the static error type of the `try` expressions in the `do` block (here `HttpError`), never an untyped `Error`.
- It must be the last clause. Like `default` in `match`, it's a **compile error when unreachable**, meaning every case is already handled.
- It's also how a typed error becomes a value (`results = .failed(error)` for a `LoadState`).
- The same pattern works in `match`: `case let other { … }` binds the whole value and acts as `default`.

## Why

- Wrapping at layer boundaries is the design's main error-handling idiom, and it needs the original error to wrap.
- `let` already means "bind a name from a pattern" ([01-principles.md](../01-principles.md)), so no new keyword is needed.
