/// The production greeter.
public service QuietGreeter implements Greeter {
    func greet(name: String) -> String {
        return "hello, {name}"
    }
}
