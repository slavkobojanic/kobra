import k.db.Database
import k.http.HttpResponse
import k.http.IncomingRequest

/// The readiness endpoint used by Docker's health checks.
///
/// It answers `200 OK` only when the database answers too, so
/// `docker compose up --wait` and load balancers send traffic to the app
/// only once it can actually serve it. It is an endpoint page with a fixed
/// URL because the `Dockerfile` and `docker-compose.yml` depend on it.
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness

    /// Reports whether the app can serve requests.
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        guard await readiness.isReady() else {
            return HttpResponse(status: .serviceUnavailable)
                .body(.text("database unavailable"))
        }

        return HttpResponse(status: .ok)
            .body(.text("ok"))
    }
}

/// Checks the dependencies the app can't work without.
service Readiness {
    @Inject let database: Database

    func isReady() async -> Bool {
        return await database.ping(timeout: 2.seconds)
    }
}
