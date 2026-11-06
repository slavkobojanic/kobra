// The generated `Endpoint` enum is server-only, so client code can't name it.
import k.http.HttpResponse
import k.http.IncomingRequest

page Health at "/health" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

component HealthLink {
    view {
        text { "{Endpoint.health.url}" }
    }
}
