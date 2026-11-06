// A local may reuse the name of a prelude function such as `untracked`; the local wins in its scope.
func compute() -> Int {
    let untracked = 3
    return untracked
    //     ^^^^^^^^^ resolves: local untracked 3:9
}
