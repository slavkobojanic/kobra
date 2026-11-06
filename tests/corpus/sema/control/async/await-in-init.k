// An `init` can't be `async` (the grammar has no `async` on `init`), so an
// `await` in its body is K2205.
func fetchStart() async -> Int {
    return 1
}

struct Counter {
    let value: Int

    init(seed: Int) {
        value = await fetchStart() + seed
    }
}
