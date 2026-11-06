// An endpoint page can't inject a repository directly; business logic stays in services.
import k.http.HttpResponse
import k.http.IncomingRequest

entity Event {
    @Id let id: Int
}

repository EventRepository for Event {}

page Webhook at "/webhook" {
    @Inject let events: EventRepository

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}
