// A `var` holds exactly its own type: a narrower collection isn't accepted
// in a `var` binding, an assignment, a `var` field's initializer or a `var`
// field's memberwise argument (K2001 with a help), while `let` accepts it.
// A wider type never converts to a narrower one, a map's keys and a
// project's own generic types are invariant.
interface Animal {
    func name() -> String
}

class Dog implements Animal {
    func name() -> String {
        return "dog"
    }
}

struct Box<T> {
    let value: T
}

struct Kennel {
    var animals: List<Animal>
    let visitors: List<Animal>
}

class Yard {
    var animals: List<Animal> = Yard.dogs
    static let dogs: List<Dog> = []
}

func check(dogs: List<Dog>, animals: List<Animal>, byNumber: Map<Int, String>, box: Box<Dog>) {
    var held: List<Animal> = dogs
    var other: List<Animal> = animals
    other = dogs
    let kennel = Kennel(animals: dogs, visitors: dogs)
    let narrow: List<Dog> = animals
    let keys: Map<Int?, String> = byNumber
    let boxed: Box<Animal> = box
}
