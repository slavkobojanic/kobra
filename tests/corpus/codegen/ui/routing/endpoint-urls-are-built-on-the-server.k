// `Endpoint` is generated per app like `Page`: one case per endpoint page,
// its path parameters as fields, and `Endpoint$href`; `PublicUrls` builds
// absolute endpoint URLs with it (12-routing.md, "Absolute URLs"; the
// app's container passes it to `$s.publicUrls`, see `server-in-tests.test.js`).
import k.http.HttpResponse
import k.http.IncomingRequest
import k.server.PublicUrls

page StripeWebhook at "/webhooks/stripe" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}

page TenantHook at "/hooks/{tenant}" {
    @Param let tenant: String

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}

// `PublicUrls` works in the browser too, but endpoint URLs don't: the
// service that builds them is server-only.
@ServerOnly
service Checkout {
    @Inject let urls: PublicUrls

    func webhookUrl() -> Url {
        return urls.absolute(endpoint: .stripeWebhook)
    }

    func tenantPath() -> Url {
        let hook: Endpoint = .tenantHook(tenant: "a b")
        return hook.url
    }
}

page Home {
    view {
        text { "home" }
    }
}
