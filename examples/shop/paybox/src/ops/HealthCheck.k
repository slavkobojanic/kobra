import k.db.Database
import k.http.HttpResponse
import k.http.IncomingRequest

/// The readiness endpoint used by Docker's and Kubernetes' health checks.
///
/// It answers `200 OK` only when the database answers too, so traffic and
/// dependent services wait until paybox can actually take payments. It is
/// an endpoint page with a fixed URL because `docker-compose.yml` and the
/// Kubernetes manifests depend on it.
public page HealthCheck at "/health" {
    @Inject let readiness: Readiness

    /// Reports whether paybox can serve requests.
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

/// Checks the dependencies paybox can't work without.
service Readiness {
    @Inject let database: Database

    func isReady() async -> Bool {
        return await database.ping(timeout: 2.seconds)
    }
}
