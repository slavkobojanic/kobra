// Accessing a member of an optional without `?.` is K2017 at the receiver, with a fix to use `?.`.
struct User {
    var name: String
}

func name(user: User?) -> String {
    return user.name ?? ""
}
