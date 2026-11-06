// Platform interfaces such as `HttpClient` are injectable only into services.
import k.http.HttpClient

component Weather {
    @Inject let http: HttpClient

    view {
        text { "weather" }
    }
}
