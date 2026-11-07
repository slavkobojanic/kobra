// A generic struct with a generic method.
struct Pair<A, B> implements Codable where A implements Codable, B implements Codable {
    let first: A
    let second: B

    func mapFirst<C>(_ transform: (A) -> C) -> Pair<C, B> where C implements Codable {
        return Pair(first: transform(first), second: second)
    }
}

func main() -> List<Pair<String, Int>> {
    let p = Pair(first: 2, second: 3)
    return [p.mapFirst { n => "n{n}" }, Pair(first: "x", second: 0)]
}
