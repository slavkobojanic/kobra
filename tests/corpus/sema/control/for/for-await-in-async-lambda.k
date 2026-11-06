// An async lambda is an async function: `for await` inside it is fine.
func run(_ block: () async -> Int) async -> Int {
    return await block()
}

func follow(changes: Stream<Int>) async -> Int {
    return await run {
        var sum = 0
        for await change in changes {
            sum += change
        }
        sum
    }
}
