// A lambda that captures a `var` keeps it alive; each factory call has its own variable.
func makeCounter() -> () -> Int {
    var count = 0
    return {
        count += 1
        count
    }
}

func main() -> List<Int> {
    let a = makeCounter()
    let b = makeCounter()
    return [a(), a(), b(), a(), b()]
}
