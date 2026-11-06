// The design's shape: a streaming call returns a `Stream`, read with `for
// await` in an async, throwing function.
enum WatchError implements Error {
    case offline
}

func watch(source: Stream<Int>) async throws WatchError -> Stream<Int> {
    return source
}

func follow(source: Stream<Int>) async throws WatchError -> Int {
    var sum = 0
    let changes = try await watch(source: source)
//      ^^^^^^^ type: Stream<Int>
    for await change in changes {
        sum += change
    }
    for await change in try await watch(source: source) {
        sum += change
    }
    return sum
}
