// A list of an interface type holds structs, enums and classes; each call dispatches to its own type.
interface Describable {
    func describe() -> String
}

struct Book implements Describable {
    let title: String

    func describe() -> String {
        return "book {title}"
    }
}

enum Coin implements Describable {
    case heads
    case tails

    func describe() -> String {
        return match self {
            case .heads { "heads" }
            case .tails { "tails" }
        }
    }
}

class Robot implements Describable {
    var id: Int = 7

    func describe() -> String {
        return "robot {id}"
    }
}

func main() -> List<String> {
    let things: List<Describable> = [Book(title: "K"), Coin.tails, Robot()]
    return things.map { thing => thing.describe() }
}
