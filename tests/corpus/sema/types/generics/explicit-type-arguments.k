// Explicit type arguments at a call site fix a generic's parameters: a
// function whose parameter appears only in its result, a method's own
// generics, a generic type's initializer and a static member of a generic
// type. Inference stays the default.
struct Box<T> {
    let value: T

    static func of(_ value: T) -> Box<T> {
        return Box(value: value)
    }
}

func empty<T>() -> List<T> {
    return []
}

func lengths(words: List<String>) -> List<Int> {
    let none = empty<String>()
  //    ^^^^ type: List<String>
    let boxed = Box<Int>(value: 3)
  //    ^^^^^ type: corpus.Box<Int>
    let made = Box<String>.of("k")
  //    ^^^^ type: corpus.Box<String>
    let inferred = Box(value: 1.5)
  //    ^^^^^^^^ type: corpus.Box<Double>
    return (none + words).map<Int> { word => word.count + boxed.value }
}
