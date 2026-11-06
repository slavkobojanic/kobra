import k.http.HttpStatus

/// Everything that can go wrong when `WeatherService` asks Open-Meteo for data.
///
/// This is the layer boundary for errors: transport failures arrive as
/// `k.http.HttpError` and are translated into these cases, so pages and components
/// handle one error type and still see what went wrong.
///
/// A failed `@Load` sends its error from the server to the browser, so no
/// case carries Open-Meteo's response body: `.upstream` keeps only the status.
public enum WeatherError implements Error, Codable {
    /// No place has the requested GeoNames id.
    case placeNotFound(id: Int)

    /// Open-Meteo could not be reached: no network, a failed DNS lookup or a
    /// refused connection.
    case offline

    /// Open-Meteo did not answer within the request timeout.
    case timeout

    /// Open-Meteo answered with an error status.
    case upstream(HttpStatus)

    /// Open-Meteo answered successfully, but the body did not have the
    /// expected shape (missing fields, series of different lengths, an
    /// unknown time zone).
    case invalidResponse(reason: String)
}
