// Equatable structs with Double fields compare exactly.
struct Sample implements Equatable {
    let value: Double
}

func main() -> List<Bool> {
    let sum = 0.1 + 0.2
    return [Sample(value: 0.5) == Sample(value: 0.5), Sample(value: sum) == Sample(value: 0.3)]
}
