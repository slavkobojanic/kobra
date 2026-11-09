/// A named location that forecasts can be requested for.
///
/// Places come from the Open-Meteo geocoding API, which uses GeoNames ids, so
/// `id` is stable across requests and safe to put in a URL. A place's name and
/// coordinates never change, which is why `WeatherService` caches places but
/// never caches forecasts.
public struct Place implements Codable, Equatable {
    /// The GeoNames id of the place, for example `2950159` for Berlin.
    let id: Int

    /// The place's name in English, for example `"Berlin"`.
    let name: String

    /// The first-level administrative area (state, province, region), when known.
    let region: String?

    /// The country name in English, when the place lies within a country.
    let country: String?

    /// Latitude in decimal degrees, positive north of the equator.
    let latitude: Double

    /// Longitude in decimal degrees, positive east of Greenwich.
    let longitude: Double

    /// The name, region and country joined for display, for example
    /// `"Berlin, Land Berlin, Germany"`.
    ///
    /// The region is left out when it repeats the name, so Lisbon reads
    /// `"Lisbon, Portugal"` rather than `"Lisbon, Lisbon, Portugal"`. Missing
    /// parts are skipped.
    var displayName: String {
        var parts: List<String> = [name]

        if let region, region != name {
            parts.append(region)
        }

        if let country {
            parts.append(country)
        }

        return parts.joined(separator: ", ")
    }

    /// The device's position as a place without a name or a GeoNames id
    /// (`id` 0), for the forecast and the air quality, which need only the
    /// coordinates. They come from a URL as text, since a URL can't carry
    /// a `Double`.
    ///
    /// - Throws: `WeatherError.placeNotFound` when either isn't a number.
    static func here(latitude: String, longitude: String) throws WeatherError -> Place {
        guard let lat = Double.parse(latitude), let lon = Double.parse(longitude) else {
            throw WeatherError.placeNotFound(id: 0)
        }

        return Place(id: 0, name: "Your location", region: nil, country: nil, latitude: lat, longitude: lon)
    }
}
