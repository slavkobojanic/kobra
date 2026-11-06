/// A named location that forecasts can be requested for.
///
/// Places come from the Open-Meteo geocoding API, which uses GeoNames ids, so
/// `id` is stable across requests and safe to put in a URL. A place's name and
/// coordinates never change, which is why `WeatherService` caches places but
/// never caches forecasts.
public struct Place implements Codable, Equatable {
    /// The GeoNames id of the place, for example `2950159` for Berlin.
    public let id: Int

    /// The place's name in English, for example `"Berlin"`.
    public let name: String

    /// The first-level administrative area (state, province, region), when known.
    public let region: String?

    /// The country name in English, when the place lies within a country.
    public let country: String?

    /// Latitude in decimal degrees, positive north of the equator.
    public let latitude: Double

    /// Longitude in decimal degrees, positive east of Greenwich.
    public let longitude: Double

    /// The name, region and country joined for display, for example
    /// `"Berlin, Land Berlin, Germany"`.
    ///
    /// The region is left out when it repeats the name, so Lisbon reads
    /// `"Lisbon, Portugal"` rather than `"Lisbon, Lisbon, Portugal"`. Missing
    /// parts are skipped.
    public var displayName: String {
        var parts: List<String> = [name]

        if let region {
            if region != name {
                parts.append(region)
            }
        }

        if let country {
            parts.append(country)
        }

        return parts.joined(separator: ", ")
    }
}
