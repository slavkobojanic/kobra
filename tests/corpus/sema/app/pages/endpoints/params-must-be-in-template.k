// Every `@Param` of an endpoint page appears in its template, optional ones included, because the
// query string is read from `request.query`.
import k.http.HttpResponse
import k.http.IncomingRequest

page TenantHook at "/webhooks/{tenant}" {
    @Param let tenant: String

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}

page Feed at "/feed" {
    @Param let limit: Int = 10

    @Get
    func list(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
