// An `if` whose branches are single mutating calls runs only the branch taken: the copy-on-write
// work of the other branch (`b.remove(at:)` on an empty list) must not run, before or after.
func main() -> List<Int> {
    var a = [1, 2, 3]
    var b: List<Int> = []
    let fromA = true
    if fromA {
        a.remove(at: 0)
    } else {
        b.remove(at: 0)
    }
    let taken = if fromA {
        a.remove(at: 0)
    } else {
        b.remove(at: 0)
    }
    if not fromA {
        b.append(9)
    } else {
        a.append(taken)
    }
    return [a.count, b.count, a[0], a[1]]
}
