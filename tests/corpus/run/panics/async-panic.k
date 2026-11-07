// A panic in an awaited async function ends the program like a synchronous one.
import k.observe.Log

func load(n: Int) async -> Int {
    Log.info("loading {n}")
    return 10 / n
}

func main() async -> Int {
    let a = await load(n: 2)
    let b = await load(n: 0)
    return a + b
}
