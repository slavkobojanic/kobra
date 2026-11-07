import k.http.HttpResponse
import k.http.IncomingRequest

/// Server-only: an endpoint page.
public page LedgerHook at "/hooks/split-endpoint-path" {
    @Inject let ledger: LedgerService

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        guard ledger.authorize(candidate: request.headers["authorization"] ?? "") else {
            return HttpResponse(status: .unauthorized)
                .body(.text("ENDPOINT_LITERAL_denied_8e8e"))
        }

        return HttpResponse(status: .noContent)
    }
}
