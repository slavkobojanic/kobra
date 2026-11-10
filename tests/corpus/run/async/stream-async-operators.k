// `filter`, `map` and `prefix(while:)` take async functions, called one
// element at a time and in order; a synchronous function is passed as is.
// `prefix(while:)` ends at the first element it refuses, dropping it.
import k.rpc.Broadcast

func isOdd(_ n: Int) -> Bool {
    return n % 2 == 1
}

func doubled(_ n: Int) async -> Int {
    return n * 2
}

func isSmall(_ n: Int) async -> Bool {
    return n < 8
}

func main() async -> List<Int?> {
    let feed: Broadcast<String, Int> = Broadcast()
    let raw = feed.subscribe(key: "k")
    for n in 1...7 {
        feed.publish(key: "k", value: n)
    }
    let small = raw
        .filter(isOdd)
        .map { n => await doubled(n) }
        .prefix(while: { n => await isSmall(n) })
    var seen: List<Int?> = []
    for await n in small {
        seen.append(n)
    }
    seen.append(await small.next())
    return seen
}
