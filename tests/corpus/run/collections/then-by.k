// `thenBy` and `thenByDescending` break ties of the previous sort.
struct Person {
    let city: String
    let age: Int
    let name: String
}

func main() -> List<List<String>> {
    let people = [Person(city: "Oslo", age: 30, name: "a"), Person(city: "Bergen", age: 30, name: "b"), Person(city: "Oslo", age: 20, name: "c"), Person(city: "Bergen", age: 40, name: "d")]
    let up = people.sortedBy { p => p.city }.thenBy { p => p.age }
    let down = people.sortedBy { p => p.city }.thenByDescending { p => p.age }
    return [up.map { p => p.name }, down.map { p => p.name }]
}
