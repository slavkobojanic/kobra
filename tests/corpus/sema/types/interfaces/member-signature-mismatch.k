// A member with the requirement's name but another type is K2015 at the member, labelled at the requirement.
interface Shape {
    func area() -> Double
}

struct Square implements Shape {
    let side: Int

    func area() -> Int {
        return side * side
    }
}
