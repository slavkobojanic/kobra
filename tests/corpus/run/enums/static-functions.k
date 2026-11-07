// Enums can have static functions that return cases.
enum Level {
    case low
    case high

    static func from(score: Int) -> Level {
        return if score > 50 { .high } else { .low }
    }
}

func main() -> List<String> {
    return ["{Level.from(score: 10)}", "{Level.from(score: 90)}"]
}
