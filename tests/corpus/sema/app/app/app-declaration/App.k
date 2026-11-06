// `App.k` holds one `app`, with annotations for app-wide defaults and a `metadata` block.
import home.Shell

/// The demo shop.
@Layout(Shell)
//      ^^^^^ resolves: decl home.Shell
@SignInPage(.signIn)
//           ^^^^^^ resolves: member Page.signIn
@Rendering(.server)
public app Shop {
    metadata {
        title("Shop")
    }
}
