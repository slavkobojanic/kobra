// A tuple pattern destructures each element of `enumerated()`; `_` ignores
// the element.
func describe(names: List<String>) -> Int {
    var sum = 0
    for (index, name) in names.enumerated() {
//       ^^^^^ type: Int
//              ^^^^ type: String
        sum += index + name.count
    }
    for _ in 0..<5 {
        sum += 1
    }
    return sum
}
