// The operand may be any expression: a member access or a call keeps its text.
struct User {
    let isAdmin: Bool

    func isActive() -> Bool {
        return true
    }
}

func check(user: User) -> Bool {
    if user.isAdmin == true {
        return user.isActive() == false
    }
    return false
}
