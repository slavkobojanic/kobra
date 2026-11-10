// `PublicUrls` is shared, since `absolute(_ page:)` works in the browser,
// but `absolute(endpoint:)` is server-only like endpoint pages: a shared
// service's methods can run in the browser, so calling it there is K4007.
// The page form is fine anywhere.
import k.http.HttpResponse
import k.http.IncomingRequest
import k.server.PublicUrls

page StripeWebhook at "/webhooks/stripe" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}

page Home at "/" {
    view {
        text { "Home" }
    }
}

service Links {
    @Inject let urls: PublicUrls

    func webhookUrl() -> Url {
        return urls.absolute(endpoint: .stripeWebhook)
    }

    func homeUrl() -> Url {
        return urls.absolute(.home)
    }
}
