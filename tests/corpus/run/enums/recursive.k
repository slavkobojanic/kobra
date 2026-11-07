// An enum may contain itself through a payload; recursion walks it.
enum Expr {
    case number(Int)
    case add(Expr, Expr)
    case multiply(Expr, Expr)
}

func evaluate(expr: Expr) -> Int {
    return match expr {
        case .number(let n) { n }
        case .add(let a, let b) { evaluate(expr: a) + evaluate(expr: b) }
        case .multiply(let a, let b) { evaluate(expr: a) * evaluate(expr: b) }
    }
}

func main() -> Int {
    let expr: Expr = .add(.number(2), .multiply(.number(3), .number(4)))
    return evaluate(expr: expr)
}
