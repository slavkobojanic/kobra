// An unresolved call in a `@Load` initializer reports K1201 only: no cycle, codability or error-type
// diagnostics follow.
page Stats {
    @Load let total: Int = try await fetchTotl()

    view {
        text { "{total}" }
    }

    loading {
    }
}
