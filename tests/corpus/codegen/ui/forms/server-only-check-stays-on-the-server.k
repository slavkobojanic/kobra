// A `@Check` whose service the browser doesn't have (a server-only one,
// here because it holds a secret) runs on the server only: the client's
// field spec has no rule for it, and no clock is passed.
import k.core.Secret
import k.validation.Check
import k.validation.Validatable

config Vault {
    @Env("VAULT_KEY") let key: Secret<String>
}

@ServerOnly
service Registry {
    @Inject let vault: Vault

    func slugFree(slug: String) async -> Bool {
        return not vault.key.matches(slug)
    }
}

struct Claim implements Codable, Validatable {
    @Check(Registry.slugFree, message: "That name is taken")
    var slug: String
}

component ClaimForm {
    @Form var claim: Claim = .empty

    view {
        form {
            textField(value: bind claim.slug)
                .label("Name")
        }
    }
}
