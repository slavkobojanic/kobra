// An interface's default method is used when a type doesn't provide one and replaced when it does.
interface Greeter {
    func name() -> String

    func greeting() -> String {
        return "Hello, {name()}"
    }
}

struct Person implements Greeter {
    let first: String

    func name() -> String {
        return first
    }
}

struct Robot implements Greeter {
    func name() -> String {
        return "R2"
    }

    func greeting() -> String {
        return "Beep, {name()}"
    }
}

func main() -> List<String> {
    return [Person(first: "Ana").greeting(), Robot().greeting()]
}
