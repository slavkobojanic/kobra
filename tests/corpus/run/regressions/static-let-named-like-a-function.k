// A `static let` may share its name with a function of its type (K2019
// exempts functions): a struct's module constant and a class's static
// property are renamed, so neither replaces nor is read as the function.
struct Tally {
    let n: Int

    static let count: Int = 10

    func count() -> Int {
        return n
    }

    static func count(of t: Tally) -> Int {
        return t.n * 2
    }
}

class Shelf {
    static let label: String = "static label"

    static func label() -> String {
        return "static func"
    }

    func label(n: Int) -> String {
        return "method {n}"
    }
}

func main() -> List<String> {
    let t = Tally(n: 3)
    return ["{Tally.count}", "{t.count()}", "{Tally.count(of: t)}", Shelf.label, Shelf.label(), Shelf().label(n: 1)]
}
