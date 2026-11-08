import k.http.HttpResponse
import k.http.IncomingRequest

/// Answers load balancers.
public page Health at "/health" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
