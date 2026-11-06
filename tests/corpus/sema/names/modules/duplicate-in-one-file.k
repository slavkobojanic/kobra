// Two declarations with one name in a module are K1004 at the later one, also within one file.
struct Item {
    let id: Int
}

struct Item {
    let name: String
}
