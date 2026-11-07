// A generic function constrained to Equatable compares values of any such type, structs included.
struct Code implements Equatable {
    let value: String
}

func indexOf<T>(_ item: T, in items: List<T>) -> Int where T implements Equatable {
    var index = 0
    for candidate in items {
        if candidate == item {
            return index
        }
        index += 1
    }
    return -1
}

func main() -> List<Int> {
    return [indexOf(3, in: [1, 2, 3]), indexOf("z", in: ["a"]), indexOf(Code(value: "b"), in: [Code(value: "a"), Code(value: "b")])]
}
