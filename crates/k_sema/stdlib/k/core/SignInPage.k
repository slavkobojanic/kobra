/// Names the page the [`k.auth.Authenticated`] guard redirects to, on the
/// `app`: `@SignInPage(.signIn)` (`docs/design/26-auth-and-sessions.md`).
@Annotation
public struct SignInPage {
    /// Makes `page` the sign-in page.
    @Native
    init(_ page: Page) {}
}
