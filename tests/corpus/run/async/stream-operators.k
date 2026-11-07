// Stream `map`, `filter`, `startingWith` and `next` are lazy and keep order.
import k.rpc.Broadcast

func main() async -> List<Int?> {
    let feed: Broadcast<String, Int> = Broadcast()
    let raw = feed.subscribe(key: "k")
    for n in 1...4 {
        feed.publish(key: "k", value: n)
    }
    let evensTimesTen = raw.filter { n => n % 2 == 0 }.map { n => n * 10 }.startingWith(0)
    let a = await evensTimesTen.next()
    let b = await evensTimesTen.next()
    let c = await evensTimesTen.next()
    return [a, b, c]
}
