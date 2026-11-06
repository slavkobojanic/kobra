import k.http.HttpClient
import k.http.HttpRequest

/// Fetches places, forecasts and air quality from the Open-Meteo APIs.
///
/// `WeatherService` is the only code in the app that talks to the network. It
/// turns Open-Meteo's wire format into the app's domain types and translates
/// every transport failure into a `WeatherError`, so pages and components
/// never see a `k.http.HttpError` directly.
///
/// Places are cached for the lifetime of the service, because a place's name
/// and coordinates never change; a place found by `searchPlaces(query:)` is
/// therefore free to look up again with `place(id:)`. Forecasts and air
/// quality are never cached: callers ask for them because they want fresh
/// data.
///
/// Open-Meteo needs no API key, so the service holds no secrets and runs in
/// the browser as well as during server rendering.
public service WeatherService {
    @Inject let http: HttpClient

    /// The shortest query, after trimming whitespace, that `searchPlaces(query:)`
    /// sends to Open-Meteo. Shorter queries match too many places to be useful.
    let minimumQueryLength: Int = 2

    private let maximumResults: Int = 8
    private let requestTimeout: Duration = 10.seconds

    private var places: Map<Int, Place> = Map()

    /// Searches for places whose name starts with `query`.
    ///
    /// - Parameter query: free text typed by the user. Leading and trailing
    ///   whitespace is ignored.
    /// - Returns: up to 8 places, best match first. The list is empty when
    ///   nothing matches, or when the trimmed query is shorter than
    ///   `minimumQueryLength`, in which case no request is made.
    /// - Throws: `WeatherError.offline`, `.timeout` or `.upstream` when
    ///   Open-Meteo can't be reached or fails, and `.invalidResponse` when its
    ///   answer can't be read.
    func searchPlaces(query: String) async throws WeatherError -> List<Place> {
        let name = query.trimmed()

        guard name.count >= minimumQueryLength else {
            return []
        }

        let response: GeocodingResponse = try await fetch(
            url: geocodingSearchUrl,
            query: ["name": name, "count": "{maximumResults}", "language": "en", "format": "json"]
        )

        guard let results = response.results else {
            return []
        }

        let found = results.map { result => result.toPlace() }

        for place in found {
            places[place.id] = place
        }

        return found
    }

    /// Looks up a single place by its GeoNames id.
    ///
    /// - Parameter id: the id from a `Place`, usually taken from the URL.
    /// - Returns: the place, from the cache when it has been seen before.
    /// - Throws: `WeatherError.placeNotFound` when Open-Meteo knows no place
    ///   with this id (it answers 400 or 404), `.offline`, `.timeout` or
    ///   `.upstream` when it can't be reached or fails otherwise, and
    ///   `.invalidResponse` when its answer can't be read.
    func place(id: Int) async throws WeatherError -> Place {
        if let cached = places[id] {
            return cached
        }

        do {
            let result: GeocodingResult = try await fetch(url: geocodingLookupUrl, query: ["id": "{id}"])
            let place = result.toPlace()

            places[id] = place

            return place
        } catch .upstream(.badRequest), .upstream(.notFound) {
            throw WeatherError.placeNotFound(id: id)
        }
    }

    /// Fetches current conditions and the next 24 hours.
    ///
    /// - Parameters:
    ///   - place: where to forecast.
    ///   - units: the unit system to request; Open-Meteo converts server-side.
    /// - Returns: a fresh forecast, never a cached one.
    /// - Throws: `WeatherError.offline`, `.timeout` or `.upstream` when
    ///   Open-Meteo can't be reached or fails, and `.invalidResponse` when its
    ///   answer can't be read or is inconsistent.
    func forecast(place: Place, units: UnitSystem) async throws WeatherError -> Forecast {
        let response: ForecastResponse = try await fetch(
            url: forecastUrl,
            query: [
                "latitude": "{place.latitude}",
                "longitude": "{place.longitude}",
                "current": currentFields,
                "hourly": hourlyFields,
                "forecast_hours": "24",
                "temperature_unit": units.temperatureParameter,
                "wind_speed_unit": units.windSpeedParameter,
                "timezone": "auto",
                "timeformat": "unixtime"
            ]
        )

        return try response.toForecast(units: units)
    }

    /// Fetches the current European Air Quality Index and particulate levels.
    ///
    /// - Parameter place: where to measure.
    /// - Returns: the current air quality. Values Open-Meteo doesn't model for
    ///   this place are `nil`; that is not an error.
    /// - Throws: `WeatherError.offline`, `.timeout` or `.upstream` when
    ///   Open-Meteo can't be reached or fails, and `.invalidResponse` when its
    ///   answer can't be read.
    func airQuality(place: Place) async throws WeatherError -> AirQuality {
        let response: AirQualityResponse = try await fetch(
            url: airQualityUrl,
            query: [
                "latitude": "{place.latitude}",
                "longitude": "{place.longitude}",
                "current": "european_aqi,pm2_5,pm10"
            ]
        )

        return response.current.toAirQuality()
    }

    /// Sends a GET request and decodes the JSON body, translating transport
    /// errors at this layer boundary. Open-Meteo's error body is dropped:
    /// `WeatherError` reaches the browser through `@Load`, and the status is
    /// all the dashboard needs.
    private func fetch<T>(url: Url, query: Map<String, String>) async throws WeatherError -> T where T implements Codable {
        let request = HttpRequest(method: .get, url: url)
            .query(query)
            .timeout(requestTimeout)

        do {
            return try await http.fetchJson(request: request)
        } catch .offline {
            throw WeatherError.offline
        } catch .timeout {
            throw WeatherError.timeout
        } catch .status(let status, _) {
            throw WeatherError.upstream(status)
        } catch .decoding(let message) {
            throw WeatherError.invalidResponse(reason: message)
        }
    }
}
