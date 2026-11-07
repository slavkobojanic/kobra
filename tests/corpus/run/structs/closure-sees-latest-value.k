// A lambda captures the variable, not a snapshot: after the `var` is reassigned the lambda reads the new struct.
struct Score {
    var points: Int
}

func main() -> List<Int> {
    var score = Score(points: 1)
    let read = { score.points }
    let first = read()
    score.points = 5
    return [first, read()]
}
