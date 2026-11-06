// A pattern binding shadows a parameter of the same name inside its arm.
enum Reading {
    case value(Int)
}

func unwrap(reading: Reading, value: Int) -> Int {
    return match reading {
        case .value(let value) {
            value
          //^^^^^ resolves: local value 8:25
        }
    }
}
