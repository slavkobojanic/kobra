// A binding inside a block is local to it; a `var` assigned in a block keeps the new value outside.
func main() -> List<Int> {
    var total = 0
    for i in 0..<3 {
        let squared = i * i
        total += squared
    }
    if total > 0 {
        let bonus = 10
        total += bonus
    }
    return [total]
}
