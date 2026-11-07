// A Duration times an Int scales it.
func main() -> List<Int> {
    let step = 2.minutes
    return [(step * 3).inSeconds, (3 * step).inSeconds, (step * 0).inSeconds]
}
