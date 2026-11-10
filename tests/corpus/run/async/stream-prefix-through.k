// `prefix(through:)` yields the elements up to and including the first
// for which its predicate is `true`, then ends and closes its source: the
// element that ends a channel's sending stream still reaches the client.
import k.rpc.Broadcast

func main() async -> List<Int?> {
    let feed: Broadcast<String, Int> = Broadcast()
    let raw = feed.subscribe(key: "k")
    for n in 1...6 {
        feed.publish(key: "k", value: n)
    }
    let upToFour = raw.prefix(through: { n => n == 4 })
    var seen: List<Int?> = []
    for await n in upToFour {
        seen.append(n)
    }
    seen.append(await upToFour.next())
    return seen
}
