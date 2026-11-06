// Like `try`, `await` covers its whole operand expression (grammar.md §13.2),
// so one `await` covers several async calls in it.
func fetchCount() async -> Int {
    return 3
}

func add(a: Int, b: Int) -> Int {
    return a + b
}

func total() async -> Int {
    return await add(a: fetchCount(), b: fetchCount())
//               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: Int
}
