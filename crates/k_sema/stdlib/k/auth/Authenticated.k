/// The guard that refuses callers who aren't signed in. On a page it
/// redirects to the page marked [`k.core.SignInPage`]; on an rpc it
/// throws `rpcFailure(.rejected(.unauthenticated))`
/// ([`GuardRejection.unauthenticated`]).
@Guard
public struct Authenticated {}
