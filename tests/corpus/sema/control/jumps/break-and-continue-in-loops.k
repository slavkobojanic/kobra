// `break` and `continue` in `for` and `while` loops, also inside nested
// `if`, `match` and `do` blocks within the loop.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func scan(values: List<Int>) -> Int {
    var total = 0
    for value in values {
        if value < 0 {
            continue
        }
        match value {
            case 0 { break }
            default { total += value }
        }
        do {
            total += try load()
        } catch _ {
            break
        }
    }
    var i = 0
    while i < 10 {
        i += 1
        if i == 5 {
            continue
        }
    }
    return total
}
