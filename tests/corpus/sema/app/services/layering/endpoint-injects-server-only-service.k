// An endpoint page may inject server-only services, such as one that injects a repository.
import k.http.HttpResponse
import k.http.IncomingRequest

entity Event {
    @Id let id: Int
}

repository EventRepository for Event {}

service EventRecorder {
    @Inject let events: EventRepository
}

page Webhook at "/webhook" {
//   ^^^^^^^ side: server-only
    @Inject let recorder: EventRecorder

    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}
