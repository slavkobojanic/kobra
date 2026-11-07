// `==` is `===` on primitives and payload-free enums and the structural `$k.equals` on structs
// (docs/compiler/codegen.md §6.1).
struct Point implements Equatable {
    let x: Int
    let y: Int
}

enum Mode {
    case on
    case off
}

func same(a: Point, b: Point) -> Bool {
    return a == b
}

func sameNumber(x: Int, y: Int) -> Bool {
    return x == y
}

func isOn(mode: Mode) -> Bool {
    return mode == .on
}

func differ(a: Point, b: Point) -> Bool {
    return a != b
}
