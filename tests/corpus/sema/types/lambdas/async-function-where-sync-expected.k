// An `async` function is not accepted where a plain function type is expected.
func runAll(step: (Int) -> Int) {}

func slowStep(n: Int) async -> Int {
    return n
}

func run() {
    runAll(step: slowStep)
}
