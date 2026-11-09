import k.auth.Session
import k.test.render
import k.text.EmailAddress

test SignUp {
    use ShelfConfig(signUpsOpen: true) for ShelfConfig

    @Inject let auth: AuthService
    @Inject let session: Session

    test "making an account signs it in" {
        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("correct horse")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect session.user != nil
    }

    test "an email address has only one account" {
        try await auth.signUp(credentials: Credentials(email: EmailAddress("ada@example.com"), password: "correct horse"))
        try await auth.signOut()

        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("Ada@example.com")
        screen.passwordField(labeled: "Password").fill("another horse")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect screen.alert().text == "There's already an account with this email address."
        expect session.user == nil
    }

    test "a password needs eight characters" {
        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("short")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect screen.text("Use at least 8 characters").isVisible
        expect session.user == nil
    }
}

test SignUpClosed {
    use ShelfConfig(signUpsOpen: false) for ShelfConfig

    @Inject let session: Session

    test "no one can make an account while sign-ups are closed" {
        let screen = render(SignUp())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("correct horse")
        screen.button(named: "Make an account").click()
        await screen.settle()

        expect screen.alert().text == "Shelf isn't taking new accounts."
        expect session.user == nil
    }
}
