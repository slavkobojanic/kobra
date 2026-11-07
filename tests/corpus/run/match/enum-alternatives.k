// One arm can list several enum cases.
enum Day {
    case monday
    case tuesday
    case saturday
    case sunday
}

func isWeekend(day: Day) -> Bool {
    return match day {
        case .saturday, .sunday { true }
        case .monday, .tuesday { false }
    }
}

func main() -> List<Bool> {
    return [isWeekend(day: .monday), isWeekend(day: .sunday), isWeekend(day: .saturday)]
}
