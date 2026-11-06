// `requires Role(…)` doesn't apply to endpoint pages either.
import k.auth.AuthRole
import k.auth.Role
import k.http.HttpResponse
import k.http.IncomingRequest

enum StaffRole implements AuthRole {
    case admin
}

page Export requires Role(.admin) at "/export" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
