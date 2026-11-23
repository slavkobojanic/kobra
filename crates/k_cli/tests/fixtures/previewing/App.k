import shop.Frame

/// The app of the preview fixture: every page in `Frame`, and a sign-in
/// page for the guarded one.
@Layout(Frame)
@SignInPage(.signIn)
public app Previewing {
    metadata {
        title("Previewing")
    }
}
