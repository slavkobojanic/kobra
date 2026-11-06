// Types in messages follow the rendering conventions: non-prelude declarations are fully qualified.
struct Item {
    let id: Int
}

let items: List<Item> = [1]
let wrong: List<String> = items
