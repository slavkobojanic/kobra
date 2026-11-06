enum Shape {
    case circle(radius: Double)
    case square(side: Double)
}

struct Labelled {
    let name: String
    var count: Int = 0
}

func describe(shape: Shape) -> String {
    match shape {
        case .circle(let radius) {
            return "circle {radius}"
        }
        case .square {
            return "square"
        }
    }
}

let sample = Labelled(name: "a")
