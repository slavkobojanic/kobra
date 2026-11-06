// `implements` naming a struct is K2014; the interface entry beside it still works.
interface Named {
    var name: String
}

struct Base {
    let id: Int
}

struct User implements Base, Named {
    let name: String
}
