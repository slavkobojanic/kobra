// `requires` takes guards: `Authenticated`, `Role(…)` of the app's `AuthRole` enum, `RateLimit(…)`.
@SignInPage(.signIn)
//          ^^^^^^^ type: Page
public app Shop {
    metadata {
        title("Shop")
    }
}
