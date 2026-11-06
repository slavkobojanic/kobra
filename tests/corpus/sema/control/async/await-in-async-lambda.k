// A lambda checked against an async function type is async: `await` inside it
// is fine even when the enclosing function is not the one awaiting.
func retry(times: Int, block: () async -> Int) async -> Int {
    return await block()
}

func fetchCount() async -> Int {
    return 3
}

func run() async -> Int {
    return await retry(times: 2) { await fetchCount() }
}
