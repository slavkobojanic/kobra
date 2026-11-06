// A label that isn't a field is K2004 in `copy(...)`.
struct User implements Copyable {
    var name: String
}

let ana = User(name: "Ana")
let renamed = ana.copy(nmae: "Anna")
