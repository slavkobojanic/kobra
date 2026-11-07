// Equality on enums with payloads, optionals of structs and lists of enums.
enum Light implements Equatable {
    case off
    case on(level: Int)
}

struct Room implements Equatable {
    let light: Light?
}

func main() -> List<Bool> {
    let dark = Room(light: .off)
    let lit = Room(light: .on(level: 2))
    let unknown = Room(light: nil)
    let lights: List<Light> = [.off, .on(level: 1)]
    return [dark == Room(light: .off), lit == Room(light: .on(level: 3)), unknown == Room(light: nil), unknown == dark, lights == [.off, .on(level: 1)]]
}
