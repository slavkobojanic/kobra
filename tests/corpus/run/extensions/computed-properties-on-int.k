// `extend Int` adds computed properties used like number extensions.
struct Percentage implements Codable {
    let value: Int
}

extend Int {
    var percent: Percentage { Percentage(value: self) }
    var squared: Int { self * self }
}

func main() -> List<String> {
    let tax = 25.percent
    return ["{tax.value}", "{7.squared}", "{(-3).squared}"]
}
