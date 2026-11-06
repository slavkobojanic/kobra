// Generic classes are instantiated like generic structs.
class Stack<T> {
    var items: List<T> = []

    func push(item: T) {
        items.append(item)
    }

    func top() -> T? {
        return items.last
    }
}

func run() -> Int? {
    let stack: Stack<Int> = Stack()
    stack.push(item: 1)
    stack.push(item: "two")
    return stack.top()
         //^^^^^^^^^^^ type: Int?
}
