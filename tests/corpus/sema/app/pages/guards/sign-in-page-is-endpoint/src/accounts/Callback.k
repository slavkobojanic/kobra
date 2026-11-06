// An endpoint page is not a `Page` case.
import k.http.HttpResponse
import k.http.IncomingRequest

public page Callback at "/callback" {
    @Get
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
