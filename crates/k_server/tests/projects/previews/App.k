import shop.Frame

/// The app of the preview bundle tests: every page in `Frame`, and a sign-in
/// page for the guarded one.
@Layout(Frame)
@SignInPage(.signIn)
public app Previews {
    metadata {
        title("Previews")
    }
}
