import k.rpc.ServiceCaller

/// The calling merchant, as its verified service token names it.
///
/// Every rpc of `pay.Payments` requires `ServiceAuthenticated`, so a call
/// from another service always has a name here; only paybox's own code,
/// calling in-process, has none.
public service VerifiedCaller implements CallingMerchant {
    @Inject let caller: ServiceCaller

    func name() -> String? {
        return caller.name
    }
}
