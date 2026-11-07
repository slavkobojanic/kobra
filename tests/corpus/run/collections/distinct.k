// `distinct` keeps first occurrences in order, comparing structs by value.
struct Tag implements Hashable, Codable {
    let name: String
}

func main() -> List<List<Tag>> {
    let tags = [Tag(name: "b"), Tag(name: "a"), Tag(name: "b"), Tag(name: "c"), Tag(name: "a")]
    let none: List<Tag> = []
    return [tags.distinct(), none.distinct()]
}
