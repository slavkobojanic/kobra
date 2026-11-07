// Durations: literals, number extensions and arithmetic agree (docs/design/07-themes-and-units.md).
func main() -> List<Bool> {
    let delay: Duration = 200ms
    return [delay == 0.2.seconds, 1.minute == 60.seconds, 2.hours + 30.minutes == 150.minutes, 1.day > 23.hours]
}
