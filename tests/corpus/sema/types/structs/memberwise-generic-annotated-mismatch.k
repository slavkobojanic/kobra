// With an annotated generic type, memberwise arguments are checked against the instantiated fields.
struct Box<T> {
    let value: T
}

let ints: Box<Int> = Box(value: 3)
let wrong: Box<Int> = Box(value: "3")
