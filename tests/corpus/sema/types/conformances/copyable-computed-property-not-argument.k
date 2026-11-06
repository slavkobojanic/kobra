// Computed properties are not `copy` arguments.
struct User implements Copyable {
    var first: String
    var full: String { "{first}!" }
}

let ana = User(first: "Ana")
let renamed = ana.copy(full: "Anna!")
