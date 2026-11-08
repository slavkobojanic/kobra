/// Desk: pages whose async work is cancelled when they unmount
/// (docs/design/03-reactivity.md, "Async work is owned"), driven by
/// `crates/k_server/tests/cancellation.rs`.
@SignInPage(.signIn)
public app Desk {
    metadata {
        title("Desk")
    }
}
