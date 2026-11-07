/// The `.test` binding of a `remote` rule whose serving app is not a member
/// of the workspace (`docs/design/40-workspaces-and-services.md`): an
/// in-process double of protocol `P` whose every rpc fails with
/// `rpcFailure(.offline)` ([`k.rpc.RpcFailure.offline`]), so a test never
/// opens a socket. A test that
/// needs answers binds a fake with `use FakePayments for Payments`.
@ServerOnly
@Native
public struct TestRemote<P> {}
