// A generic class keeps its own state per instance.
class Stack<T> {
    var items: List<T> = []

    func push(item: T) {
        items.append(item)
    }

    func pop() -> T? {
        if items.isEmpty {
            return nil
        }
        return items.remove(at: items.count - 1)
    }
}

func main() -> List<Int?> {
    let a: Stack<Int> = Stack()
    let b: Stack<Int> = Stack()
    a.push(item: 1)
    a.push(item: 2)
    b.push(item: 9)
    return [a.pop(), a.pop(), a.pop(), b.pop()]
}
