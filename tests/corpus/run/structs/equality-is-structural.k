// Equatable structs compare field by field, nested structs and lists included.
struct Tag implements Equatable {
    let name: String
}

struct Post implements Equatable {
    let title: String
    let tags: List<Tag>
}

func main() -> List<Bool> {
    let a = Post(title: "Hi", tags: [Tag(name: "x")])
    let b = Post(title: "Hi", tags: [Tag(name: "x")])
    let c = Post(title: "Hi", tags: [Tag(name: "y")])
    let d = Post(title: "Hi", tags: [])
    return [a == b, a == c, a != c, a == d, d == Post(title: "Hi", tags: [])]
}
