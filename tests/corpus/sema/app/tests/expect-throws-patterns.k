// path: src/corpus/Weather.test.k
// `expect throws` takes a pattern, nested or without its payload.
enum UpstreamError implements Error {
    case serviceUnavailable
    case timeout
}

enum WeatherError implements Error {
    case upstream(UpstreamError)
    case unknownPlace
}

func forecast(place: String) async throws WeatherError -> Int {
    throw WeatherError.upstream(.serviceUnavailable)
}

test Weather {
    test "nested pattern" {
        expect throws WeatherError.upstream(.serviceUnavailable) {
            try await forecast(place: "Berlin")
        }
    }

    test "any payload" {
        expect throws WeatherError.upstream {
            try await forecast(place: "Berlin")
        }
    }
}
