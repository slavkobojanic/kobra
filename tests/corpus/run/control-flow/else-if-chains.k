// `else if` chains try conditions in order.
func grade(score: Int) -> String {
    if score >= 90 {
        return "A"
    } else if score >= 75 {
        return "B"
    } else if score >= 50 {
        return "C"
    } else {
        return "F"
    }
}

func main() -> List<String> {
    return [grade(score: 95), grade(score: 90), grade(score: 80), grade(score: 50), grade(score: 0)]
}
