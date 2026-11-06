// Fields with `let` and `var`, with and without types and initializers.
struct User {
    let id: UserId
    var name: String
    var nickname: String? = nil
    var tags = ["new"]
    let createdAt: Date = Date.now
    var anything
}
