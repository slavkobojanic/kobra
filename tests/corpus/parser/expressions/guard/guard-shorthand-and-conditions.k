// `guard let x` shorthand and boolean conditions in one list
func f() {
    guard let user, user.isAdmin, count > 0 else { return }
}
