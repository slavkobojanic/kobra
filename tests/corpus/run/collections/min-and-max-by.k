// `minBy` and `maxBy` return the element with the extreme key, or nil for an empty list.
struct City implements Codable {
    let name: String
    let population: Int
}

func main() -> List<City?> {
    let cities = [City(name: "Oslo", population: 700), City(name: "Bergen", population: 290), City(name: "Tromso", population: 77)]
    let none: List<City> = []
    return [cities.minBy { c => c.population }, cities.maxBy { c => c.population }, none.minBy { c => c.population }]
}
