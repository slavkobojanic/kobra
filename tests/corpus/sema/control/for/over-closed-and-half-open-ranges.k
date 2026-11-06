// `a...b` and `a..<b` over `Int` count; the element is an `Int`.
func total(rows: Int) -> Int {
    var sum = 0
    for attempt in 1...3 {
//      ^^^^^^^ type: Int
//                 ^^^^^ type: ClosedRange<Int>
        sum += attempt
    }
    for row in 0..<rows {
//      ^^^ type: Int
//             ^^^^^^^^ type: HalfOpenRange<Int>
        sum += row
    }
    return sum
}
