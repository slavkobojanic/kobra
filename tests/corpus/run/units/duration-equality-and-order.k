// `200ms == 0.2.seconds`; singular and plural extensions are the same unit; durations are ordered.
func main() -> List<Bool> {
    return [200ms == 0.2.seconds, 1.hour == 60.minutes, 1.minute == 1.minutes, 90.seconds > 1.minute, 2s <= 2000ms, 1.week < 6.days]
}
