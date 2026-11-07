// Double duration extensions produce exact milliseconds.
func main() -> List<Int> {
    return [0.2.seconds.inMilliseconds, 1.5.minutes.inSeconds, 0.5.hours.inSeconds]
}
