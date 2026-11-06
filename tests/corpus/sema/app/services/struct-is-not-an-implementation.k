// Only services are injectable implementations: a struct implementing the interface doesn't count.
interface Greeter {
    func greet() -> String
}

struct PlainGreeter implements Greeter {
    func greet() -> String {
        return "hi"
    }
}

service Home {
    @Inject let greeter: Greeter
}
