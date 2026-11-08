/// The `.test` binding of every `remote` rule, also when the serving app is
/// a member of the same workspace
/// (`docs/design/40-workspaces-and-services.md`): an in-process double of
/// protocol `P` whose every rpc fails with `rpcFailure(.offline)`
/// ([`k.rpc.RpcFailure.offline`]), so each service's tests stay hermetic
/// and never open a socket. A test that needs answers binds its own fake
/// with a `use` rule: `use FakePayments for Payments`.
@ServerOnly
@Native
public struct TestRemote<P> {}
