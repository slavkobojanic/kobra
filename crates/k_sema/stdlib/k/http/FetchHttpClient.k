/// The standard [`HttpClient`], built on the web-standard `fetch`.
@Native
public service FetchHttpClient implements HttpClient {
    /// Sends `request` with `fetch`.
    ///
    /// - Parameter request: the request to send.
    /// - Returns: the response, with any status.
    /// - Throws: [`HttpError.offline`] when the server can't be reached and
    ///   [`HttpError.timeout`] when the request's timeout elapses.
    func send(request: HttpRequest) async throws HttpError -> HttpResponse
}
