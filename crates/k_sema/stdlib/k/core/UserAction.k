/// Marks a member the browser allows only while the user is acting on the
/// page (`docs/design/45-web-platform.md`, "User activation"): a file
/// picker, a share sheet, a permission prompt. It may be called only from
/// the synchronous part of a click, submit or key handler, directly or
/// through other functions, never from an effect, a load, a timer or after
/// an `await`.
///
/// It goes on an interface member, so calls through the interface are
/// checked, and on a component event, `@UserAction @Emits let pressed:
/// Event<Void>`, whose handlers may then call such members. A function
/// that calls one before its first `await` is user-action-only itself,
/// without the annotation.
@Annotation
public struct UserAction {
    /// `true` for a member that uses up the user's action, as a share
    /// sheet does: one context may make only one such call.
    let consuming: Bool = false
}
