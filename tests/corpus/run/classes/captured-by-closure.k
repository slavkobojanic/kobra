// A lambda that captures an instance mutates the shared instance on every call.
class Tally {
    var total: Int = 0
}

func main() -> Int {
    let tally = Tally()
    let add = { (n: Int) -> Int => tally.total + n }
    tally.total = add(2)
    tally.total = add(3)
    return tally.total
}
