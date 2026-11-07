// A struct may have a member named `text`: it doesn't collide with the struct's generated
// interpolation text, so both `tag.text` and "{tag}" work.
struct Tag {
    let name: String

    var text: String {
        return "#{name}"
    }
}

func main() -> List<String> {
    let tag = Tag(name: "kanban")
    return [tag.text, "{tag}"]
}
