// A `let` without initializer, assigned on every path before the read.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func pick(flag: Bool, light: Int) -> Int {
    let a: Int
    if flag {
        a = 1
    } else {
        a = 2
    }
    let b: Int
    match light {
        case 0 { b = 0 }
        default { b = 1 }
    }
    return a + b
}
