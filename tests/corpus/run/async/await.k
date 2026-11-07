// Async functions await each other in order; `main` may be async.
import k.observe.Log

func load(id: Int) async -> String {
    Log.info("loading {id}")
    return "item {id}"
}

func main() async -> List<String> {
    let first = await load(id: 1)
    let second = await load(id: 2)
    return [first, second]
}
