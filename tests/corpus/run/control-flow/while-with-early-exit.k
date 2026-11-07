// `while` re-checks its condition before each iteration and may never run.
func main() -> List<Int> {
    var n = 10
    var steps = 0
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 }
        steps += 1
    }
    var never = 0
    while never > 0 {
        never += 1
    }
    return [steps, n, never]
}
