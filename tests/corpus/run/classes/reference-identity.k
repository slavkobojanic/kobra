// Class instances are shared by reference: a change through one binding is seen through every other.
class Counter {
    var count: Int = 0
}

func main() -> List<Int> {
    let a = Counter()
    let b = a
    let c = Counter()
    b.count = 5
    c.count = 1
    return [a.count, b.count, c.count]
}
