/// Shelf: the app-wide settings. Pages, services and the theme are
/// found from their declarations, so this holds only the sign-in page,
/// where pages that need an account send everyone else, and the document
/// entries every page shares.
@SignInPage(.signIn)
public app Shelf {
    metadata {
        locale(.english)
    }
}
