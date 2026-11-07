// A `catch` clause with `where` handles only matching payloads; the rest fall through to later clauses.
enum LimitError implements Error {
    case over(by: Int)
}

func check(amount: Int) throws LimitError -> Int {
    if amount > 10 {
        throw LimitError.over(by: amount - 10)
    }
    return amount
}

func attempt(amount: Int) -> String {
    do {
        return "ok {try check(amount: amount)}"
    } catch .over(let by) where by > 5 {
        return "way over by {by}"
    } catch .over(let by) {
        return "over by {by}"
    }
}

func main() -> List<String> {
    return [attempt(amount: 3), attempt(amount: 12), attempt(amount: 30)]
}
