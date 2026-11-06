// half-open and closed ranges of literals, including negative bounds
func f() {
    match age {
        case 0..<13 { .child }
        case 13...120 { .person }
        case -5..<0 { .invalid }
    }
}
