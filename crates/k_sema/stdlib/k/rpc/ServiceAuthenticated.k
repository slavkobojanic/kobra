/// The guard that admits only other services of the system
/// (`docs/design/40-workspaces-and-services.md`): a call must carry a
/// short-lived service token signed with the secret in [`ServiceConfig`],
/// as a `remote` client sends it. A refused rpc throws
/// `rpcFailure(.rejected(.unauthenticated))`; the verified caller's name is
/// [`ServiceCaller.name`].
@Guard
public struct ServiceAuthenticated {}
