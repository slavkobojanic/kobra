// Optional chaining yields `null`, never `undefined`, when its result is stored
// (docs/compiler/codegen.md §5, §6.1).
struct User {
    let name: String
}

func nameOf(user: User?) -> String? {
    let name = user?.name
    return name
}
