/// "Sign in with a passkey", for accounts made with one.
@Rendering(.client)
public component PasskeySignIn {
    /// The page to continue to after signing in.
    @Prop let returnTo: Page

    @Inject let passkeys: PasskeyService

    @State var problem: String? = nil

    // The challenge is fetched as the page opens, so the click can open
    // the sheet at once.
    @Effect
    func prepare() async {
        await passkeys.prepareSignIn()
    }

    // Nothing while the server renders: the challenge exists only once
    // the browser has fetched it.
    loading { }

    // Shown once the challenge is in hand, so the button always works.
    view {
        if passkeys.isSupported and passkeys.signInOptions != nil {
            if let problem {
                paragraph { problem }
                    .class(.problem)
                    .role(.alert)
            }

            button { "Sign in with a passkey" }
                .on(click: signIn)
        }
    }

    func signIn() async {
        problem = await passkeys.signIn()
        if problem == nil {
            Router.navigate(to: returnTo)
        }
    }

    style {
        .problem { color: .danger; }
    }
}
