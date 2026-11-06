/// The standard `HttpClient`, built on the web-standard `fetch`.
@Native
public service FetchHttpClient implements HttpClient {
    /// Sends `request` with `fetch`.
    func send(request: HttpRequest) async throws HttpError -> HttpResponse
}
