import k.test.Screen
import k.test.TestClock
import k.text.EmailAddress

test SignUp {
    @Inject let auth: AuthService
    @Inject let clock: TestClock

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
        try await auth.signUp(request: signUpRequest(email: EmailAddress("ada@example.com")))
        try await auth.signOut()
        let screen = render(SignUp())

        fill(screen: screen, email: "Ada@example.com", password: testPassword, confirmation: testPassword)
        screen.button(named: "Create account").click()
        await screen.settle()

        expect screen.text("An account with this email already exists").isVisible
        expect auth.user == nil
    }
}

/// The email's `@Check` in the browser: leaving the field runs it, debounced,
/// before anything is submitted.
test SignUpCheck {
    @Inject let auth: AuthService
    @Inject let clock: TestClock

    test "a registered email is flagged once the field loses focus" {
        try await auth.signUp(request: signUpRequest(email: EmailAddress("ada@example.com")))
        try await auth.signOut()
        let screen = render(SignUp())
        let email = screen.textField(labeled: "Email")

        email.fill("Ada@example.com")
        email.blur()
        await screen.settle()

        expect not screen.text("An account with this email already exists").exists

        clock.advance(by: 300.milliseconds)
        await screen.settle()

        expect screen.text("An account with this email already exists").isVisible
    }

    test "a free email is not flagged" {
        let screen = render(SignUp())
        let email = screen.textField(labeled: "Email")

        email.fill("grace@example.com")
        email.blur()
        clock.advance(by: 300.milliseconds)
        await screen.settle()

        expect not screen.text("An account with this email already exists").exists
    }
}

/// Types into the sign-up form's three fields.
func fill(screen: Screen, email: String, password: String, confirmation: String) {
    screen.textField(labeled: "Email").fill(email)
    screen.passwordField(labeled: "Password").fill(password)
    screen.passwordField(labeled: "Confirm password").fill(confirmation)
}
