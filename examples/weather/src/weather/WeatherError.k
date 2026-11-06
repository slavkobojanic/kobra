import k.http.HttpError

/// Everything that can go wrong when `WeatherService` asks Open-Meteo for data.
///
/// This is the layer boundary for errors: transport failures arrive as
/// `HttpError` and are wrapped in `.network`, so pages and components handle
/// one error type and still see the underlying cause.
public enum WeatherError implements Error {
    /// No place has the requested GeoNames id.
    case placeNotFound(id: Int)

    /// Open-Meteo could not be reached, did not answer in time, or answered
    /// with an error status.
    case network(HttpError)

    /// Open-Meteo answered successfully, but the body did not have the
    /// expected shape (missing fields, series of different lengths, an
    /// unknown time zone).
    case invalidResponse(reason: String)
}
