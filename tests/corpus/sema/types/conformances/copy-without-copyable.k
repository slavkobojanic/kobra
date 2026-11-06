// Without `implements Copyable` there is no `copy`: K1203.
struct User {
    var name: String
}

let ana = User(name: "Ana")
let renamed = ana.copy(name: "Anna")
