// An async function can take an async function and await it.
func twice(_ action: (Int) async -> Int, start: Int) async -> Int {
    let once = await action(start)
    return await action(once)
}

func addTen(n: Int) async -> Int {
    return n + 10
}

func main() async -> Int {
    return await twice(addTen, start: 1)
}
