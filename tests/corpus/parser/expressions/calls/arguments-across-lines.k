// newlines inside `( )` are insignificant, with or without a trailing comma
func f() {
    let a = image(
        source: user.avatarUrl,
        description: "Avatar",
    )
    let b = f(
        1
        ,
        2)
}
