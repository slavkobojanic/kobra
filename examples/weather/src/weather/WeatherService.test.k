import k.http.HttpClient
import testing.FakeHttpClient

test WeatherService {
    use FakeHttpClient for HttpClient

    @Inject let http: FakeHttpClient
    @Inject let weather: WeatherService

    // Searching

    test "search turns geocoding results into places" {
        http.serveJson(path: "/v1/search", body: GeocodingResponse(results: [berlinResult()]))

        let places = try await weather.searchPlaces(query: "Berlin")

        expect places.count == 1
        expect places[0].id == 2950159
        expect places[0].displayName == "Berlin, Land Berlin, Germany"
    }

    test "search trims the query and asks for at most eight English results" {
        http.serveJson(path: "/v1/search", body: GeocodingResponse(results: []))

        try await weather.searchPlaces(query: "  Berlin  ")

        expect http.requests.count == 1
        expect http.requests[0].query["name"] == "Berlin"
        expect http.requests[0].query["count"] == "8"
        expect http.requests[0].query["language"] == "en"
    }

    test "a response without results means nothing matched" {
        http.serveJson(path: "/v1/search", body: GeocodingResponse(results: nil))

        let places = try await weather.searchPlaces(query: "Atlantis")

        expect places.isEmpty
    }

    test "queries shorter than two characters never reach the network" {
        let places = try await weather.searchPlaces(query: " b ")

        expect places.isEmpty
        expect http.requests.isEmpty
    }

    test "a search that times out is a network error" {
        http.fail(path: "/v1/search", error: .timeout)

        expect throws WeatherError.network(.timeout) {
            try await weather.searchPlaces(query: "Berlin")
        }
    }

    // Looking up a place

    test "an unknown place id is a typed not-found error" {
        http.serveText(path: "/v1/get", body: "Invalid id", status: 400)

        expect throws WeatherError.placeNotFound {
            try await weather.place(id: 42)
        }
    }

    test "places found by a search are looked up from the cache" {
        http.serveJson(path: "/v1/search", body: GeocodingResponse(results: [berlinResult()]))

        try await weather.searchPlaces(query: "Berlin")
        let place = try await weather.place(id: 2950159)

        expect place.name == "Berlin"
        expect http.requests.count == 1
    }

    test "a place lookup is cached after the first request" {
        http.serveJson(path: "/v1/get", body: berlinResult())

        try await weather.place(id: 2950159)
        try await weather.place(id: 2950159)

        expect http.requests.count == 1
    }

    // Forecasts

    test "a forecast response becomes a forecast in the place's time zone" {
        http.serveJson(path: "/v1/forecast", body: forecastResponse())

        let forecast = try await weather.forecast(place: .berlin, units: .metric)

        expect forecast.current.temperature == 8.1
        expect forecast.current.condition == .partlyCloudy
        expect forecast.current.isDay
        expect forecast.hourly.count == 3
        expect forecast.hourly[1].time == Date(unixSeconds: 1793970000)
        expect forecast.timeZone.identifier == "Europe/Berlin"
        expect forecast.updatedAt == Date(unixSeconds: 1793966400)
    }

    test "metric forecasts request Celsius and kilometres per hour" {
        http.serveJson(path: "/v1/forecast", body: forecastResponse())

        try await weather.forecast(place: .berlin, units: .metric)

        expect http.requests[0].query["temperature_unit"] == "celsius"
        expect http.requests[0].query["wind_speed_unit"] == "kmh"
        expect http.requests[0].query["latitude"] == "52.52437"
    }

    test "imperial forecasts request Fahrenheit and miles per hour" {
        http.serveJson(path: "/v1/forecast", body: forecastResponse())

        let forecast = try await weather.forecast(place: .berlin, units: .imperial)

        expect forecast.units == .imperial
        expect http.requests[0].query["temperature_unit"] == "fahrenheit"
        expect http.requests[0].query["wind_speed_unit"] == "mph"
    }

    test "forecasts are never cached" {
        http.serveJson(path: "/v1/forecast", body: forecastResponse())

        try await weather.forecast(place: .berlin, units: .metric)
        try await weather.forecast(place: .berlin, units: .metric)

        expect http.requests.count == 2
    }

    test "hourly series of different lengths are an invalid response" {
        http.serveJson(path: "/v1/forecast", body: forecastResponse(hourlyTemperatures: [8.1, 7.6]))

        expect throws WeatherError.invalidResponse {
            try await weather.forecast(place: .berlin, units: .metric)
        }
    }

    test "an unknown time zone is an invalid response" {
        http.serveJson(path: "/v1/forecast", body: forecastResponse(timezone: "Mars/Olympus_Mons"))

        expect throws WeatherError.invalidResponse {
            try await weather.forecast(place: .berlin, units: .metric)
        }
    }

    test "a body that is not the expected JSON is an invalid response" {
        http.serveText(path: "/v1/forecast", body: "<html>Bad gateway</html>")

        expect throws WeatherError.invalidResponse {
            try await weather.forecast(place: .berlin, units: .metric)
        }
    }

    test "a server error status is a network error" {
        http.serveText(path: "/v1/forecast", body: "Service unavailable", status: 503)

        expect throws WeatherError.network {
            try await weather.forecast(place: .berlin, units: .metric)
        }
    }

    test "being offline is a network error" {
        http.fail(path: "/v1/forecast", error: .offline)

        expect throws WeatherError.network(.offline) {
            try await weather.forecast(place: .berlin, units: .metric)
        }
    }

    // Air quality

    test "air quality reports the European index and its category" {
        http.serveJson(path: "/v1/air-quality", body: airQualityResponse(europeanIndex: 45))

        let airQuality = try await weather.airQuality(place: .berlin)

        expect airQuality.europeanIndex == 45
        expect airQuality.category == .moderate
    }

    test "air quality without a European index is unavailable, not an error" {
        http.serveJson(path: "/v1/air-quality", body: airQualityResponse(europeanIndex: nil))

        let airQuality = try await weather.airQuality(place: .berlin)

        expect airQuality.category == .unavailable
    }
}

// Wire-format builders. They live in this file because the wire structs are
// package-private to the `weather` module.

func berlinResult() -> GeocodingResult {
    return GeocodingResult(
        id: 2950159,
        name: "Berlin",
        latitude: 52.52437,
        longitude: 13.41053,
        country: "Germany",
        region: "Land Berlin"
    )
}

func forecastResponse(
    hourlyTemperatures: List<Double> = [8.1, 7.6, 7.2],
    timezone: String = "Europe/Berlin"
) -> ForecastResponse {
    return ForecastResponse(
        timezone: timezone,
        current: CurrentResponse(
            time: 1793966400,
            temperature: 8.1,
            feelsLike: 5.9,
            humidity: 81,
            windSpeed: 14.2,
            windDirection: 250.0,
            weatherCode: 2,
            isDay: 1
        ),
        hourly: HourlyResponse(
            time: [1793966400, 1793970000, 1793973600],
            temperature: hourlyTemperatures,
            feelsLike: [5.9, 5.3, 4.8],
            precipitationChance: [10, 15, 30]
        )
    )
}

func airQualityResponse(europeanIndex: Int?) -> AirQualityResponse {
    return AirQualityResponse(
        current: AirQualityCurrent(europeanIndex: europeanIndex, pm25: 6.4, pm10: 11.0)
    )
}
