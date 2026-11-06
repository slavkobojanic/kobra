// `@Get` and the other method annotations belong to endpoint page handlers only.
import k.http.HttpResponse
import k.http.IncomingRequest

service Feed {
    @Get
    func list(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
