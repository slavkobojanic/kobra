// An async call in a function that isn't async is K2204 (it must be awaited);
// the fix isn't machine-applicable there, because `await` alone would then be
// K2205. Only one diagnostic: no K2205, since there is no `await`.
func fetchCount() async -> Int {
    return 3
}

func doubled() -> Int {
    return fetchCount() * 2
}
