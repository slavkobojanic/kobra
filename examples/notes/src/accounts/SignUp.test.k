import k.test.Screen

test SignUp {
    @Inject let auth: AuthService

    test "a valid sign-up continues to the notes list, signed in" {
        let screen = render(SignUp())

        fill(screen: screen, email: "ada@example.com", password: testPassword, confirmation: testPassword)
        screen.button(named: "Create account").click()
        await screen.settle()

        expect screen.currentPage == .notes
        expect auth.user != nil
    }

    test "mismatched passwords are reported before anything is sent" {
        let screen = render(SignUp())

        fill(screen: screen, email: "ada@example.com", password: testPassword, confirmation: "correct horse")
        screen.button(named: "Create account").click()
        await screen.settle()

        expect screen.text("Passwords must match").isVisible
        expect screen.currentPage == .signUp
        expect auth.user == nil
    }

    test "a short password is reported under its field" {
        let screen = render(SignUp())

        fill(screen: screen, email: "ada@example.com", password: "short", confirmation: "short")
        screen.button(named: "Create account").click()
        await screen.settle()

        expect screen.text("Use at least 8 characters").isVisible
    }

    test "an email that already has an account is flagged under the email field" {
        try await auth.signUp(request: signUpRequest(email: "ada@example.com"))
        await auth.signOut()
        let screen = render(SignUp())

        fill(screen: screen, email: "Ada@example.com", password: testPassword, confirmation: testPassword)
        screen.button(named: "Create account").click()
        await screen.settle()

        expect screen.text("An account with this email already exists").isVisible
        expect auth.user == nil
    }
}

/// Types into the sign-up form's three fields.
func fill(screen: Screen, email: String, password: String, confirmation: String) {
    screen.textField(labeled: "Email").fill(email)
    screen.passwordField(labeled: "Password").fill(password)
    screen.passwordField(labeled: "Confirm password").fill(confirmation)
}
