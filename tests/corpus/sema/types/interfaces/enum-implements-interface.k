// Enums can implement interfaces with methods.
interface Labelled {
    func label() -> String
}

enum Size implements Labelled {
    case small
    case large

    func label() -> String {
        return match self {
            case .small { "S" }
            case .large { "L" }
        }
    }
}

func show(item: Labelled) -> String {
    return item.label()
}

let shown = show(item: Size.small)
  //^^^^^ type: String
