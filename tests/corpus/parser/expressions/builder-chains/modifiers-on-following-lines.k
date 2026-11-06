// required values as named arguments, optional values chained on following lines
func f() {
    image(source: user.avatarUrl, description: "Profile photo of {user.name}")
        .class(.avatar)
        .loading(.lazy)
}
