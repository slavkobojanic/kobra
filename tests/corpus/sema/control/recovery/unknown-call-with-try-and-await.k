// `try await` on an unresolved callee: K1201, no needless-`try` warning and
// no K2203 for the unknown error (the `await` in a sync function is K2205).
func show() -> Int {
    return try await fetchCount()
}
