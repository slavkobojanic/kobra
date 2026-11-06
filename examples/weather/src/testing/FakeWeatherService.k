import k.time.Clock
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
/// (on the injected `Clock`, which in a test is `k.test.TestClock`, so no
/// real time passes) or make every call fail, to reach the `loading` and
/// `failed` states deterministically.
///
/// This module is test-only (see `docs/design/21-testing.md`).
public service FakeWeatherService implements WeatherService {
    @Inject let clock: Clock

    /// The same minimum as the real service, so components behave identically.
    let minimumQueryLength: Int = 2

    private let knownPlaces: List<Place> = [.berlin, .lisbon]
    private var latency: Duration = 0ms
    private var failure: WeatherError? = nil

    /// Every query passed to `searchPlaces(query:)`, oldest first.
    var searchQueries: List<String> = []

    /// How many times `forecast(place:units:)` has been called.
    var forecastCalls: Int = 0

    /// How many times `airQuality(place:)` has been called.
    var airQualityCalls: Int = 0

    /// Makes every later call wait before answering.
    ///
    /// - Parameter duration: how long each call waits, in virtual time on the
    ///   injected `Clock`.
    func simulateLatency(duration: Duration) {
        latency = duration
    }

    /// Makes every later call throw `error` after its latency.
    ///
    /// - Parameter error: the error to throw.
    func simulateFailure(error: WeatherError) {
        failure = error
    }

    /// Undoes `simulateFailure(error:)`, so later calls succeed again.
    func clearFailure() {
        failure = nil
    }

    /// Returns the known places whose name starts with the trimmed query,
    /// ignoring case.
    func searchPlaces(query: String) async throws WeatherError -> List<Place> {
        searchQueries.append(query)

        try await simulateNetwork()

        let prefix = query.trimmed().lowercased()

        return knownPlaces.filter { place => place.name.lowercased().startsWith(prefix) }
    }

    /// Returns the known place with this id.
    ///
    /// - Throws: `WeatherError.placeNotFound` for any other id.
    func place(id: Int) async throws WeatherError -> Place {
        try await simulateNetwork()

        let found = knownPlaces.firstWhere { place => place.id == id }

        guard let found else {
            throw WeatherError.placeNotFound(id: id)
        }

        return found
    }

    /// Returns `Forecast.sample(units:)` for any place.
    func forecast(place: Place, units: UnitSystem) async throws WeatherError -> Forecast {
        forecastCalls += 1

        try await simulateNetwork()

        return .sample(units: units)
    }

    /// Returns `AirQuality.sample` for any place.
    func airQuality(place: Place) async throws WeatherError -> AirQuality {
        airQualityCalls += 1

        try await simulateNetwork()

        return .sample
    }

    private func simulateNetwork() async throws WeatherError {
        if latency > 0ms {
            await clock.sleep(latency)
        }

        if let failure {
            throw failure
        }
    }
}
