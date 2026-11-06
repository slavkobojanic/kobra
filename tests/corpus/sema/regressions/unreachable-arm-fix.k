// K2302 offers a maybe-incorrect fix removing the arm (diagnostics.md, control
// flow).
enum L {
    case a
    case b
}

func m(l: L) -> Int {
    return match l {
        case .a { 1 }
        case .b { 2 }
        default { 3 }
    }
}
