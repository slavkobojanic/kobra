// A TimeZone is a value: equal by zone, usable as a set element, and
// encoded as its IANA name (35-time.md).
import k.observe.Log
import k.time.TimeZone

struct Forecast implements Codable {
    let place: String
    let timeZone: TimeZone
}

func main() -> List<Forecast> {
    let berlin = TimeZone.parse("Europe/Berlin") ?? TimeZone.utc
    let again = TimeZone.parse("Europe/Berlin") ?? TimeZone.utc
    let zones: Set<TimeZone> = [berlin, again, TimeZone.utc]
    Log.info("{berlin == again} {berlin == TimeZone.utc} {zones.count}")
    return [Forecast(place: "Berlin", timeZone: berlin), Forecast(place: "Null Island", timeZone: TimeZone.utc)]
}
