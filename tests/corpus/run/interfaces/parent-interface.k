// An interface that implements another requires both; a value of the child type can be used as the parent.
interface Named {
    func name() -> String
}

interface Titled implements Named {
    func title() -> String
}

struct Doctor implements Titled {
    func name() -> String {
        return "Ana"
    }

    func title() -> String {
        return "Dr. {name()}"
    }
}

func nameOf(_ value: Named) -> String {
    return value.name()
}

func main() -> List<String> {
    let d = Doctor()
    return [nameOf(d), d.title()]
}
