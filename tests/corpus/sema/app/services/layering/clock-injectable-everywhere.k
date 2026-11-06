// `Clock` is a stdlib service: components, pages, layouts, services of every scope and endpoint pages
// may inject it.
import k.http.HttpResponse
import k.http.IncomingRequest
import k.time.Clock

component Now {
    @Inject let clock: Clock
    //                 ^^^^^ resolves: decl k.time.Clock

    view {
        text { "{clock.now}" }
        //       ^^^^^^^^^ type: Date
    }
}

page Today {
    @Inject let clock: Clock

    view {
        Now()
    }
}

layout Frame {
    @Inject let clock: Clock
    @Slot let content: View

    view {
        content
    }
}

@Scope(.server)
service Uptime {
    @Inject let clock: Clock
}

page Ping at "/ping" {
    @Inject let clock: Clock

    @Get
    func ping(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
