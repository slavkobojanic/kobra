// path: src/corpus/Weather.test.k
// The block of `expect throws E` must be able to throw `E`.
enum WeatherError implements Error {
    case unknownPlace
}

enum OtherError implements Error {
    case broken
}

func degrees() -> Int {
    return 20
}

func forecast(place: String) async throws WeatherError -> Int {
    throw WeatherError.unknownPlace
}

test Weather {
    test "cannot throw at all" {
        expect throws WeatherError.unknownPlace {
            let value = degrees()
        }
    }

    test "throws another type" {
        expect throws OtherError.broken {
            try await forecast(place: "Nowhere")
        }
    }
}
