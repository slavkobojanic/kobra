// An endpoint page can't inject a protocol.
import k.http.HttpResponse
import k.http.IncomingRequest
import k.rpc.RpcOnlyError

protocol CartProtocol {
    rpc items() throws RpcOnlyError -> List<Int>
}

page Webhook at "/webhook" {
    @Inject let api: CartProtocol

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}
