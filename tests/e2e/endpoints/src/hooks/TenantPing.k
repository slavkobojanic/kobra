import k.http.HttpResponse
import k.http.IncomingRequest

/// A typed path parameter: a value that isn't an `Int` is a 404.
public page TenantPing at "/tenants/{tenant}/ping" {
    @Param let tenant: Int

    @Get
    func ping(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text("pong from tenant {tenant}"))
    }
}
