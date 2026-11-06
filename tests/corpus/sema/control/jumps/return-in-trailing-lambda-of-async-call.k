// A transaction-style block is a lambda: `marked` is its value, and
// `return marked` is K2308 even though the block is async.
func transaction<T>(_ block: () async -> T) async -> T {
    return await block()
}

func mark() async -> Bool {
    let marked = await transaction {
        let marked = true
        return marked
    }
    return marked
}
