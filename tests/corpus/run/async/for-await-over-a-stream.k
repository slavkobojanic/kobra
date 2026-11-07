// `for await` reads a stream's values in order; `break` stops reading.
import k.observe.Log
import k.rpc.Broadcast

func main() async -> List<Int> {
    let feed: Broadcast<String, Int> = Broadcast()
    let stream = feed.subscribe(key: "prices")
    feed.publish(key: "prices", value: 1)
    feed.publish(key: "other", value: 100)
    feed.publish(key: "prices", value: 2)
    feed.publish(key: "prices", value: 3)
    var seen: List<Int> = []
    for await price in stream {
        Log.info("price {price}")
        seen.append(price)
        if seen.count == 3 {
            break
        }
    }
    return seen
}
