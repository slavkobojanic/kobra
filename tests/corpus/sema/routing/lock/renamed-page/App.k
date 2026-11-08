// Renaming `Pricing` to `Plans` moves its URL; the rename is detected, but the
// redirect from `/pricing` isn't recorded yet: K8007 asks for `k build`.
public app Demo {
    metadata {
        title("Demo")
    }
}
