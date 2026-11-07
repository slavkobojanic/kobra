// A declared `init` replaces the memberwise initializer and may compute fields.
struct Temperature implements Codable {
    let celsius: Int
    let label: String

    init(celsius: Int) {
        self.celsius = celsius
        self.label = if celsius < 0 { "freezing" } else { "fine" }
    }
}

func main() -> List<Temperature> {
    return [Temperature(celsius: -5), Temperature(celsius: 20)]
}
