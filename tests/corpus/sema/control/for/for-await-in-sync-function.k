// `for await` over a `Stream` outside an async function is K2311 at `await`
// (sema.md §11.2: only inside an async function). There is no fix.
func follow(changes: Stream<Int>) -> Int {
    var sum = 0
    for await change in changes {
        sum += change
    }
    return sum
}
