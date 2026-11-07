import k.rpc.RemoteClient
import k.rpc.RpcFailure
import k.rpc.ServiceConfig
import pay.Charge
import pay.PaymentEvents
import pay.Refund

/// Calls merchants over the network, with a service token signed by
/// `CallbackConfig.token` (or paybox's own `K_SERVICE_TOKEN` without one).
///
/// Merchants register their addresses at run time, so there is no `remote`
/// rule for `PaymentEvents` in `App.k`: `RemoteClient.at` builds a client
/// for each call from the registered URL, and the protocol it calls is the
/// type the client is declared as.
public service RemoteNotifier implements MerchantNotifier {
    @Inject let config: CallbackConfig
    @Inject let service: ServiceConfig

    func chargeSucceeded(_ charge: Charge, at callbackUrl: Url) async -> RpcFailure? {
        let events: PaymentEvents = RemoteClient.at(url: callbackUrl, token: config.token ?? service.token, timeout: config.timeout)

        do {
            try await events.chargeSucceeded(charge: charge)
        } catch .rpcFailure(let failure) {
            return failure
        }

        return nil
    }

    func chargeRefunded(_ refund: Refund, at callbackUrl: Url) async -> RpcFailure? {
        let events: PaymentEvents = RemoteClient.at(url: callbackUrl, token: config.token ?? service.token, timeout: config.timeout)

        do {
            try await events.chargeRefunded(refund: refund)
        } catch .rpcFailure(let failure) {
            return failure
        }

        return nil
    }
}
