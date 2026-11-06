// At most one handler per HTTP method.
import k.http.HttpResponse
import k.http.IncomingRequest

page Feed at "/feed" {
    @Get
    func list(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }

    @Get
    func again(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }

    @Post
    func add(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .created)
    }
}
