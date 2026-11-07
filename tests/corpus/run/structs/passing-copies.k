// A struct passed to a function is a copy: the callee's changes to its own `var` don't reach the caller.
struct Counter {
    var count: Int
}

func bumped(counter: Counter) -> Counter {
    var local = counter
    local.count += 1
    return local
}

func main() -> List<Int> {
    let start = Counter(count: 1)
    let next = bumped(counter: start)
    let again = bumped(counter: next)
    return [start.count, next.count, again.count]
}
