// A nil before `?.` skips the whole rest of the chain: plain links after it
// (`a?.b.c`), method calls, computed properties, stdlib members, indexes and a
// second `?.` further on. None of them is read on nil.
import k.observe.Log

struct Tag {
    let label: String

    func shout() -> String {
        Log.info("shouting {label}")
        return label.uppercased()
    }
}

struct Pet {
    let name: String
    let tag: Tag
    let tags: List<Tag>
    let owner: Pet?

    var title: String {
        return "Pet {name}"
    }
}

func row(pet: Pet?) -> List<String?> {
    return [
        pet?.tag.label,
        pet?.tag.shout(),
        pet?.tag.label.uppercased(),
        pet?.title.lowercased(),
        pet?.tags[0].label,
        pet?.owner?.tag.label,
        pet?.tag.shout().lowercased(),
    ]
}

func counts(pet: Pet?) -> List<Int?> {
    return [pet?.name.count, pet?.tags.count, pet?.owner?.tags.count]
}

func main() -> List<List<String?>> {
    let rex = Pet(name: "Rex", tag: Tag(label: "good"), tags: [Tag(label: "first")], owner: nil)
    let ann = Pet(name: "Ann", tag: Tag(label: "kind"), tags: [Tag(label: "one")], owner: rex)
    let nobody: Pet? = nil
    let c = counts(pet: nobody)
    let d = counts(pet: ann)
    return [row(pet: ann), row(pet: nobody), ["{c[0] ?? -1} {c[1] ?? -1} {c[2] ?? -1}", "{d[0] ?? -1} {d[1] ?? -1} {d[2] ?? -1}"]]
}
