// Endpoint pages share the URL space with view pages.
import k.http.HttpResponse
import k.http.IncomingRequest

page Health {
    view {
        text { "health" }
    }
}

page HealthCheck at "/health" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
