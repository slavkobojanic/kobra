/// The development greeter.
public service LoudGreeter implements Greeter {
    func greet(name: String) -> String {
        return "HELLO, {name}!"
    }
}
