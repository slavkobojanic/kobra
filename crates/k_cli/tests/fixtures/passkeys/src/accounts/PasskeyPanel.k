/// Sign up, out and in with a passkey, each from a click; the options
/// for the next ceremony are fetched before it.
public component PasskeyPanel {
    @Inject let flow: PasskeyFlow

    @State var name: String = ""
    @State var said: String = ""

    @Effect
    func prepare() async {
        await flow.prepareSignUp()
        said = "ready"
    }

    view {
        textField(value: bind name)
            .label("Name")

        button { "Sign up with a passkey" }
            .on(click: signUp)

        button { "Sign out" }
            .on(click: signOut)

        button { "Sign in with a passkey" }
            .on(click: signIn)

        paragraph { said }
            .role(.status)
    }

    func signUp() async {
        said = await flow.signUp(name: name)
    }

    func signOut() async {
        await flow.signOut()
        await flow.prepareSignIn()
        said = "signed out"
    }

    func signIn() async {
        said = await flow.signIn()
    }
}
