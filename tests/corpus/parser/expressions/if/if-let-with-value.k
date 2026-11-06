// `if let x = e` binds the unwrapped optional
func f() {
    if let user = session.user { greet(user) }
}
