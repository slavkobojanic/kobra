// Endpoint pages are cases of the server-only `Endpoint` enum, used by server code.
import k.http.HttpResponse
import k.http.IncomingRequest
import k.server.PublicUrls

page StripeWebhook at "/webhooks/stripe" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}

// `PublicUrls` is shared, but endpoint URLs are server-only, so the
// service that builds them is too.
@ServerOnly
service Checkout {
    @Inject let urls: PublicUrls

    func webhookUrl() -> Url {
        return urls.absolute(endpoint: .stripeWebhook)
        //     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: Url
        //                              ^^^^^^^^^^^^^ resolves: member Endpoint.stripeWebhook
    }

    func path() -> Url {
        return Endpoint.stripeWebhook.url
    }
}
