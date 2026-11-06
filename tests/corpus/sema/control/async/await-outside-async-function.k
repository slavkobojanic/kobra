// `await` in a function that isn't `async` is K2205 at the `await` token.
func fetchCount() async -> Int {
    return 3
}

func doubled() -> Int {
    let count = await fetchCount()
    return count * 2
}
