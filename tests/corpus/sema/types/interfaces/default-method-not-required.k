// A method with a default body need not be implemented, and is callable on the conforming type.
interface Greeter {
    var name: String

    func greeting() -> String {
        return "Hello, {name}"
    }
}

struct Person implements Greeter {
    let name: String
}

let text = Person(name: "Ana").greeting()
  //^^^^ type: String
                             //^^^^^^^^ resolves: member corpus.Greeter.greeting()
