// After an unknown member, the rest of the chain reports nothing.
struct User {
    let name: String
}

func length(user: User) -> Int {
    return user.nme.count + 1
}
