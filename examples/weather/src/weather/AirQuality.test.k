test AirQuality {
    test "each band starts at its lower bound" {
        expect airQualityAt(index: 0).category == .good
        expect airQualityAt(index: 20).category == .fair
        expect airQualityAt(index: 40).category == .moderate
        expect airQualityAt(index: 60).category == .poor
        expect airQualityAt(index: 80).category == .veryPoor
        expect airQualityAt(index: 100).category == .extremelyPoor
    }

    test "each band ends just below the next one" {
        expect airQualityAt(index: 19).category == .good
        expect airQualityAt(index: 39).category == .fair
        expect airQualityAt(index: 59).category == .moderate
        expect airQualityAt(index: 79).category == .poor
        expect airQualityAt(index: 99).category == .veryPoor
    }

    test "values far above the scale are extremely poor" {
        expect airQualityAt(index: 480).category == .extremelyPoor
    }

    test "a missing index is unavailable" {
        expect AirQuality(europeanIndex: nil, pm25: 4.0, pm10: 9.0).category == .unavailable
    }

    test "a negative index is treated as invalid" {
        expect airQualityAt(index: -1).category == .unavailable
    }

    test "labels are written out in full" {
        expect AirQualityCategory.veryPoor.label == "Very poor"
        expect AirQualityCategory.unavailable.label == "Unavailable"
    }
}

func airQualityAt(index: Int) -> AirQuality {
    return AirQuality(europeanIndex: index, pm25: nil, pm10: nil)
}
