import k.http.HttpResponse
import k.http.IncomingRequest

/// The health endpoint `dist/app probe` checks.
public page Health at "/health" {
    @Inject let greeter: Greeter

    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text("ok: {greeter.greeting()}"))
    }
}
