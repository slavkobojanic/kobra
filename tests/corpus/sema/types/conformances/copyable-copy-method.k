// `Copyable` adds `copy(...)` with every field optional, `let` fields included; the result is the struct.
struct User implements Copyable {
    let id: Int
    var name: String
    var email: String
}

let ana = User(id: 1, name: "Ana", email: "a@example.com")
let renamed = ana.copy(name: "Anna")
  //^^^^^^^ type: corpus.User
                //^^^^ resolves: member corpus.User.copy(id:name:email:)
let same = ana.copy()
  //^^^^ type: corpus.User
let other = ana.copy(id: 2, email: "b@example.com")
