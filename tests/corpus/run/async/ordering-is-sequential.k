// Awaits run in order: each async call finishes before the code after its `await` continues.
import k.observe.Log

func step(name: String) async -> String {
    Log.info("start {name}")
    let inner = await finish(name: name)
    Log.info("end {name}")
    return inner
}

func finish(name: String) async -> String {
    Log.info("finish {name}")
    return name.uppercased()
}

func main() async -> List<String> {
    Log.info("main")
    let a = await step(name: "a")
    let b = await step(name: "b")
    Log.info("done")
    return [a, b]
}
