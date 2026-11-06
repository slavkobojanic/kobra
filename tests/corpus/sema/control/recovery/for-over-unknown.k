// An unresolved iterable is K1201 only: no K2310 or K2311, and the loop
// variable's uses are not errors.
func run() async -> Int {
    var sum = 0
    for value in values {
        sum += value
    }
    for await item in items {
        sum += item
    }
    return sum
}
