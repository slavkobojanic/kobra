// `if let`, `guard let`, `??` and optional chaining on nil and non-nil values.
struct User {
    let name: String
    let nickname: String?
}

func display(user: User?) -> String {
    guard let user else {
        return "nobody"
    }
    if let nickname = user.nickname {
        return nickname
    }
    return user.name
}

func main() -> List<String> {
    let ana = User(name: "Ana", nickname: "A")
    let ben = User(name: "Ben", nickname: nil)
    let none: User? = nil
    return [display(user: ana), display(user: ben), display(user: none), none?.name ?? "default"]
}
