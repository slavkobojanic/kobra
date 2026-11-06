/// The guard that refuses callers who aren't signed in. On a page it
/// redirects to the `@SignInPage`; on an rpc it throws
/// `rpcFailure(.rejected(.unauthenticated))`.
@Guard
public struct Authenticated {}
