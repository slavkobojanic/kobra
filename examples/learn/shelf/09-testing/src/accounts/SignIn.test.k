import k.auth.Session
import k.test.render
import k.text.EmailAddress

test SignIn {
    @Inject let auth: AuthService
    @Inject let session: Session

    test "the right password signs in" {
        try await auth.signUp(credentials: Credentials(email: EmailAddress("ada@example.com"), password: "correct horse"))
        try await auth.signOut()

        let screen = render(SignIn())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("correct horse")
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect session.user != nil
    }

    test "a wrong password doesn't" {
        try await auth.signUp(credentials: Credentials(email: EmailAddress("ada@example.com"), password: "correct horse"))
        try await auth.signOut()

        let screen = render(SignIn())

        screen.textField(labeled: "Email").fill("ada@example.com")
        screen.passwordField(labeled: "Password").fill("wrong horse")
        screen.button(named: "Sign in").click()
        await screen.settle()

        expect screen.alert().text == "That email address and password don't match an account."
        expect session.user == nil
    }
}
