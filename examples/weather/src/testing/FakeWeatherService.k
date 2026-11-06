import weather.AirQuality
import weather.Forecast
import weather.Place
import weather.UnitSystem
import weather.WeatherError
import weather.WeatherService

/// An in-memory stand-in for `WeatherService`, for page and component tests.
///
/// It knows two places, `.berlin` and `.lisbon`, answers every forecast with
/// `Forecast.sample(units:)`, and counts its calls. Tests can make it slow
/// (with a `TestClock`, so no real time passes) or make every call fail, to
/// reach the `loading` and `failed` states deterministically.
///
/// This module is test-only (see `docs/design/proposals/testing-support.md`).
public service FakeWeatherService implements WeatherService {
    /// The same minimum as the real service, so components behave identically.
    public let minimumQueryLength: Int = 2

    private let knownPlaces: List<Place> = [.berlin, .lisbon]
    private var latency: Duration = 0ms
    private var failure: WeatherError? = nil

    /// Every query passed to `searchPlaces(query:)`, oldest first.
    public var searchQueries: List<String> = []

    /// How many times `forecast(place:units:)` has been called.
    public var forecastCalls: Int = 0

    /// How many times `airQuality(place:)` has been called.
    public var airQualityCalls: Int = 0

    /// Makes every later call wait before answering.
    ///
    /// - Parameter duration: how long each call waits, measured on the
    ///   injected `Clock`.
    public func simulateLatency(duration: Duration) {
        latency = duration
    }

    /// Makes every later call throw `error` after its latency.
    ///
    /// - Parameter error: the error to throw.
    public func simulateFailure(error: WeatherError) {
        failure = error
    }

    /// Undoes `simulateFailure(error:)`, so later calls succeed again.
    public func clearFailure() {
        failure = nil
    }

    /// Returns the known places whose name starts with the trimmed query,
    /// ignoring case.
    public func searchPlaces(query: String) async throws WeatherError -> List<Place> {
        searchQueries.append(query)

        try await simulateNetwork()

        let prefix = query.trimmed().lowercased()

        return knownPlaces.filter { place => place.name.lowercased().startsWith(prefix) }
    }

    /// Returns the known place with this id.
    ///
    /// - Throws: `WeatherError.placeNotFound` for any other id.
    public func place(id: Int) async throws WeatherError -> Place {
        try await simulateNetwork()

        let found = knownPlaces.firstWhere { place => place.id == id }

        guard let found else {
            throw WeatherError.placeNotFound(id: id)
        }

        return found
    }

    /// Returns `Forecast.sample(units:)` for any place.
    public func forecast(place: Place, units: UnitSystem) async throws WeatherError -> Forecast {
        forecastCalls += 1

        try await simulateNetwork()

        return .sample(units: units)
    }

    /// Returns `AirQuality.sample` for any place.
    public func airQuality(place: Place) async throws WeatherError -> AirQuality {
        airQualityCalls += 1

        try await simulateNetwork()

        return .sample
    }

    private func simulateNetwork() async throws WeatherError {
        if latency > 0ms {
            await Timer.sleep(latency)
        }

        if let failure {
            throw failure
        }
    }
}
