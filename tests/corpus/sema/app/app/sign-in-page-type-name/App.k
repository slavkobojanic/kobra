// `@SignInPage` takes a `Page` value (`.signIn`), not the page's type name.
import accounts.SignIn

@SignInPage(SignIn)
public app Shop {
    metadata {
        title("Shop")
    }
}
