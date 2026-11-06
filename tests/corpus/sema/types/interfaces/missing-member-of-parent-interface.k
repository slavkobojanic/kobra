// `interface A implements B` makes every implementation of `A` also provide `B`'s members.
interface Identified {
    var id: Int
}

interface Named implements Identified {
    var name: String
}

struct Tag implements Named {
    let name: String
}

struct Label implements Named {
    let id: Int
    let name: String
}
