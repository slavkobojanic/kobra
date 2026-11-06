import k.http.HttpResponse
import k.http.IncomingRequest

page Health {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

page Ready at "/ready" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
