import k.http.HttpResponse
import k.http.IncomingRequest

/// Reports what the handler sees of the request.
public page Inspect at "/inspect" {
    @Get
    func show(request: IncomingRequest) async -> HttpResponse {
        let multi = request.headers["x-multi"] ?? "none"
        let all = request.headers.all("x-multi")
        let query = request.query["q"] ?? "none"

        return HttpResponse(status: .ok)
            .body(.text("multi=[{multi}] count={all.count} q=[{query}] address={request.clientAddress} url={request.url}"))
    }
}
