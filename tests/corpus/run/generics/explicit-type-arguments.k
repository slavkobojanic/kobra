// Explicit type arguments at call sites: a generic function whose type
// parameter appears only in its result, a method's own generics, and a
// generic type's initializer. They produce exactly what inference does.
struct Box<T> {
    let value: T
}

struct Tagged<T> {
    let tag: String
    let items: List<T>
}

func empty<T>() -> List<T> {
    return []
}

func second<A, B>(_ a: A, _ b: B) -> B {
    return b
}

func main() -> List<String> {
    let none = empty<Int>()
    let words = empty<String>() + ["k"]
    let boxed = Box<Int>(value: 3)
    let tagged = Tagged<String>(tag: "t", items: words)
    let lengths = tagged.items.map<Int> { word => word.count }
    let one = second<Int, String>(1, "one")
    return ["{none.count}", words[0], "{boxed.value}", "{lengths[0]}", one]
}
