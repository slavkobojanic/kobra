test WeatherCondition {
    test "single codes map to their condition" {
        expect conditionFor(wmoCode: 0) == .clear
        expect conditionFor(wmoCode: 3) == .overcast
    }

    test "every intensity of a precipitation type shares one condition" {
        for code in [51, 53, 55, 56, 57] {
            expect conditionFor(wmoCode: code) == .drizzle
        }

        for code in [61, 63, 65, 66, 67] {
            expect conditionFor(wmoCode: code) == .rain
        }

        for code in [71, 73, 75, 77] {
            expect conditionFor(wmoCode: code) == .snow
        }
    }

    test "showers count as rain or snow" {
        expect conditionFor(wmoCode: 80) == .rain
        expect conditionFor(wmoCode: 82) == .rain
        expect conditionFor(wmoCode: 85) == .snow
        expect conditionFor(wmoCode: 86) == .snow
    }

    test "thunderstorms with hail are still thunderstorms" {
        expect conditionFor(wmoCode: 96) == .thunderstorm
        expect conditionFor(wmoCode: 99) == .thunderstorm
    }

    test "codes outside the WMO table are unknown rather than an error" {
        expect conditionFor(wmoCode: 4) == .unknown
        expect conditionFor(wmoCode: -1) == .unknown
        expect conditionFor(wmoCode: 100) == .unknown
    }

    test "clear and partly cloudy skies show the moon at night" {
        expect WeatherCondition.clear.icon(isDay: true) == .sun
        expect WeatherCondition.clear.icon(isDay: false) == .moon
        expect WeatherCondition.partlyCloudy.icon(isDay: false) == .cloudMoon
    }

    test "other conditions look the same by day and night" {
        expect WeatherCondition.rain.icon(isDay: true) == WeatherCondition.rain.icon(isDay: false)
    }
}
