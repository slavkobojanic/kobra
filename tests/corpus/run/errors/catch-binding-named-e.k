// A K binding named `e` never collides with the JavaScript catch variable:
// `catch let e`, a catch-all over two enums, a nested `do` binding the same
// name, a binding that shadows an outer `e`, and an `e` declared inside a
// clause or a `try … else` fallback.
enum LimitError implements Error {
    case over(by: Int)
    case under
}

enum OtherError implements Error {
    case broken
}

func check(n: Int) throws LimitError -> Int {
    if n > 10 {
        throw LimitError.over(by: n - 10)
    }
    if n < 0 {
        throw LimitError.under
    }
    return n
}

func other(n: Int) throws OtherError -> Int {
    if n == 5 {
        throw OtherError.broken
    }
    return n
}

func describe(_ error: LimitError) -> String {
    return match error {
        case .over(let by) { "over by {by}" }
        case .under { "under" }
    }
}

func single(n: Int) -> String {
    do {
        return "ok {try check(n: n)}"
    } catch let e {
        return describe(e)
    }
}

func two(n: Int) -> String {
    do {
        let a = try check(n: n)
        return "ok {try other(n: a)}"
    } catch .under {
        return "under"
    } catch let e {
        let errors: List<Error> = [e]
        return "caught {errors.count}"
    }
}

func nested(n: Int) -> String {
    do {
        return "ok {try check(n: n)}"
    } catch let e {
        let first = describe(e)
        do {
            return "{first}, then ok {try check(n: n - 45)}"
        } catch let e {
            return "{first}, then {describe(e)}"
        }
    }
}

func shadowing(n: Int) -> String {
    let e = "outer"
    do {
        return "{e} ok {try check(n: n)}"
    } catch let e {
        return describe(e)
    }
}

func declared(n: Int) -> String {
    do {
        return "ok {try check(n: n)}"
    } catch .over(let by) {
        let e = by * 2
        return "doubled {e}"
    } catch .under {
        return "under"
    }
}

func fallback(n: Int) -> String {
    let v = try check(n: n) else {
        let e = "fell back"
        return e
    }
    return "ok {v}"
}

func main() -> List<String> {
    return [
        single(n: 3), single(n: 12),
        two(n: 4), two(n: 5), two(n: -1), two(n: 11),
        nested(n: 2), nested(n: 15), nested(n: 50),
        shadowing(n: 1), shadowing(n: 20),
        declared(n: 13), declared(n: 2),
        fallback(n: 30), fallback(n: 6),
    ]
}
