import k.text.EmailAddress

test SignIn {
    @Inject let auth: AuthService

    test "the right password continues to the page that sent the visitor here" {
        try await auth.signUp(request: signUpRequest(email: EmailAddress("ada@example.com")))
        try await auth.signOut()
        let screen = render(SignIn().returnTo(.newNote))

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill(testPassword)
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect screen.currentPage == .newNote
    }

    test "an email typed with spaces around it and in another letter case signs in" {
        try await auth.signUp(request: signUpRequest(email: EmailAddress("ada@example.com")))
        try await auth.signOut()
        let screen = render(SignIn())

        screen.textField(labeled: "Email").fill("  Ada@Example.COM ")
        screen.passwordField(labeled: "Password").fill(testPassword)
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect screen.currentPage == .notes
        expect auth.user != nil
    }

    test "a wrong password shows an alert and stays on the page" {
        try await auth.signUp(request: signUpRequest(email: EmailAddress("ada@example.com")))
        try await auth.signOut()
        let screen = render(SignIn())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("not the password")
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect screen.alert().text == "That email and password don't match an account."
        expect screen.currentPage == .signIn
        expect auth.user == nil
    }

    test "an invalid email and an empty password are caught before anything is sent" {
        let screen = render(SignIn())

        screen.textField(labeled: "Email").fill("not an email")
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect screen.text("Enter a valid email address").isVisible
        expect screen.text("Enter your password").isVisible
        expect not screen.alert().exists
    }
}
