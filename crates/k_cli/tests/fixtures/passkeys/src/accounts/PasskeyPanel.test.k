import k.test.TestAuthenticator

test PasskeyPanel {
    @Inject let authenticator: TestAuthenticator

    test "the clicks sign up, out and in" {
        let screen = render(PasskeyPanel())
        await screen.settle()
        expect screen.text("ready").isVisible

        screen.textField(labeled: "Name").fill("ada")
        screen.button(named: "Sign up with a passkey").click()
        await screen.settle()
        expect screen.text("signed up").isVisible

        screen.button(named: "Sign out").click()
        await screen.settle()
        expect screen.text("signed out").isVisible

        screen.button(named: "Sign in with a passkey").click()
        await screen.settle()
        expect screen.text("signed in as ada").isVisible
        expect authenticator.prompts == 2
    }
}
