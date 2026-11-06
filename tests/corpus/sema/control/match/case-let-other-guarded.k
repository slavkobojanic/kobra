// A guarded `case let other where …` is not a catch-all: guards never count,
// so the match is still missing values (K2301), and the arm isn't K2304 since
// the guard uses the binding.
func size(count: Int) -> String {
    return match count {
        case 0 { "empty" }
        case let other where other > 10 { "many" }
    }
}
