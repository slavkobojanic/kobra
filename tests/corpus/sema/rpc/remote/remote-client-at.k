// `RemoteClient.at` builds a client of the protocol its result is used as
// (40-workspaces-and-services.md); the result must be a protocol.
import k.rpc.RemoteClient
import k.rpc.RpcOnlyError

protocol MerchantEvents {
    rpc paid(chargeId: String) throws RpcOnlyError
}

struct Hook {
    let url: Url
}

@Scope(.server)
service Notifier {
//      ^^^^^^^^ side: server-only
    func notify(url: Url, token: Secret<String>) async throws RpcOnlyError {
        let events: MerchantEvents = RemoteClient.at(url: url, token: token)
        //                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: corpus.MerchantEvents
        try await events.paid(chargeId: "ch_1")
        let quick: MerchantEvents = RemoteClient.at(url: url, token: token, timeout: 2.seconds)
        try await quick.paid(chargeId: "ch_2")
    }

    func wrong(url: Url, token: Secret<String>) {
        let hook: Hook = RemoteClient.at(url: url, token: token)
        let unknown = RemoteClient.at(url: url, token: token)
    }
}
