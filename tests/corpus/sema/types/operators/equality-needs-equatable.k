// `==` needs `Equatable`: without it, K2013; with a derived `Equatable`, fine.
struct Plain {
    let x: Int
}

struct Compared implements Equatable {
    let x: Int
}

func check(a: Plain, b: Plain, c: Compared, d: Compared) -> Bool {
    let same = c == d
      //^^^^ type: Bool
    return a == b
}
