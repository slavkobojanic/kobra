// A method called through `?.` on nil is not called at all.
import k.observe.Log

struct Greeter {
    let name: String

    func greet() -> String {
        Log.info("greeting {name}")
        return "hi {name}"
    }
}

func main() -> List<String?> {
    let some: Greeter? = Greeter(name: "Ana")
    let none: Greeter? = nil
    return [some?.greet(), none?.greet()]
}
