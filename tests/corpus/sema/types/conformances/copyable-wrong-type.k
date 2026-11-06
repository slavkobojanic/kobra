// `copy` arguments have the fields' types.
struct User implements Copyable {
    var name: String
}

let ana = User(name: "Ana")
let renamed = ana.copy(name: 3)
