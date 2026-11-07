import k.http.HttpResponse
import k.http.IncomingRequest

/// Shows the config, redacted, and checks candidates against the secrets.
public page ConfigPage at "/config/{what}" {
    @Param let what: String

    @Inject let reporter: ConfigReporter

    @Get
    func show(request: IncomingRequest) async -> HttpResponse {
        let candidate = request.query["candidate"] ?? ""

        if what == "token" {
            let verdict = if reporter.tokenMatches(candidate: candidate) { "match" } else { "no match" }

            return HttpResponse(status: .ok)
                .body(.text(verdict))
        }

        if what == "key" {
            let verdict = if reporter.keyMatches(candidate: candidate) { "match" } else { "no match" }

            return HttpResponse(status: .ok)
                .body(.text(verdict))
        }

        return HttpResponse(status: .ok)
            .body(.text(reporter.describe()))
    }
}
