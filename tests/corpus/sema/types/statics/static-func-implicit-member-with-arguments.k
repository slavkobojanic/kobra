// A static factory returning the type is an implicit member with arguments; wrong arguments are checked.
struct Shade {
    let red: Int

    static func gray(level: Int) -> Shade {
        return Shade(red: level)
    }
}

let a: Shade = .gray(level: 3)
let b: Shade = .gray(level: "x")
