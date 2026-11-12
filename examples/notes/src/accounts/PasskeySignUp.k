import k.text.EmailAddress

/// "Sign up with a passkey": an email, then the device's passkey sheet.
/// The account it creates has no password.
@Rendering(.client)
public component PasskeySignUp {
    @Inject let passkeys: PasskeyService

    @State var email: String = ""
    @State var problem: String? = nil

    // The challenge is fetched as the page opens, so the click can open
    // the sheet at once.
    @Effect
    func prepare() async {
        await passkeys.prepareSignUp()
    }

    // Nothing while the server renders: the challenge exists only once
    // the browser has fetched it.
    loading {}

    // Shown once the challenge is in hand, so the button always works.
    view {
        if passkeys.isSupported and passkeys.signUpOptions != nil {
            section {
                heading(level: 2) { "Or use a passkey" }

                textField(value: bind email)
                    .label("Email for the passkey")

                if let problem {
                    paragraph { problem }
                        .class(.problem)
                        .role(.alert)
                }

                button { "Sign up with a passkey" }
                    .on(click: signUp)
            }
            .class(.passkey)
        }
    }

    func signUp() async {
        guard let address = EmailAddress.parse(email) else {
            problem = "Enter an email address, such as ada@example.com."
            return
        }

        problem = await passkeys.signUp(email: address)
        if problem == nil {
            Router.navigate(to: .notes)
        }
    }

    style {
        .passkey { display: grid; gap: .md; maxWidth: 24rem; marginTop: .lg; }
        .problem { color: .danger; }
    }
}
