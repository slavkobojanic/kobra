import k.http.HttpResponse
import k.http.IncomingRequest

/// Decodes a JSON body; a decoding failure is the handler's own 400.
public page JsonHook at "/hooks/json" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let event: OrderEvent = try request.json() else {
            return HttpResponse(status: .badRequest)
                .body(.text("not an order event"))
        }

        return HttpResponse(status: .ok)
            .body(.json(event))
    }
}
