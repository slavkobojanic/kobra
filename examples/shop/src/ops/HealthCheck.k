import k.db.Database
import k.http.HttpRequest
import k.http.HttpResponse

/// The readiness endpoint used by Docker's health checks.
///
/// It answers `200 OK` only when the database answers too, so
/// `docker compose up --wait` and load balancers send traffic to the app
/// only once it can actually serve it. The URL is named because the
/// `Dockerfile` and `docker-compose.yml` depend on it.
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness

    /// Reports whether the app can serve requests.
    @Get
    func check(request: HttpRequest) async -> HttpResponse {
        guard await readiness.isReady() else {
            return HttpResponse(status: 503, body: "database unavailable")
        }

        return HttpResponse(status: 200, body: "ok")
    }
}

/// Checks the dependencies the app can't work without.
service Readiness {
    @Inject let database: Database

    func isReady() async -> Bool {
        return await database.ping(timeout: 2.seconds)
    }
}
