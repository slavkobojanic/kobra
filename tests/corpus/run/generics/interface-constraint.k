// A constraint on an interface calls the implementation of each type.
interface Shape {
    func area() -> Int
}

struct Square implements Shape {
    let side: Int

    func area() -> Int {
        return side * side
    }
}

struct Strip implements Shape {
    let length: Int

    func area() -> Int {
        return length
    }
}

func totalArea<T>(_ shapes: List<T>) -> Int where T implements Shape {
    var total = 0
    for shape in shapes {
        total += shape.area()
    }
    return total
}

func main() -> List<Int> {
    return [totalArea([Square(side: 2), Square(side: 3)]), totalArea([Strip(length: 7)])]
}
