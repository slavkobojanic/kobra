// Durations add, subtract and negate; literal suffixes and extensions agree.
func main() -> List<Int> {
    let total = 10.minutes + 30.seconds
    let mixed = 1.second + 500ms
    let back = -5.minutes
    let less = 2s - 500ms
    return [total.inSeconds, mixed.inMilliseconds, back.inSeconds, less.inMilliseconds, 3.weeks.inSeconds, 1.day.inSeconds]
}
