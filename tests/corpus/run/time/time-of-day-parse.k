// `TimeOfDay.parse` accepts exactly what a `TimeOfDay("…")` literal does (20-standard-library.md,
// "Literals and parsing"): `HH:MM` on a 24-hour clock, two digits each. A decoded TimeOfDay is
// checked by the same parser, so other text is a `mismatch`, never a value.
import k.encoding.Json
import k.http.DecodingError
import k.time.TimeOfDay

struct Alarm implements Codable {
    let at: TimeOfDay
}

func parsed(_ text: String) -> String {
    if let t = TimeOfDay.parse(text) {
        return "{t.hour}:{t.minute}"
    }
    return "nil"
}

func decoded(_ text: String) -> String {
    do {
        let alarm: Alarm = try Json.decode(text)
        return "ok {alarm.at == TimeOfDay("07:05")}"
    } catch .malformed {
        return "malformed"
    } catch .mismatch {
        return "mismatch"
    }
}

func main() -> List<String> {
    return [
        parsed("07:05"),
        parsed("00:00"),
        parsed("23:59"),
        parsed("24:00"),
        parsed("12:60"),
        parsed("7:05"),
        parsed(" 07:05"),
        parsed("07:05:00"),
        parsed("noon"),
        parsed(""),
        decoded("{{\"at\": \"07:05\"}"),
        decoded("{{\"at\": \"25:00\"}"),
        decoded("{{\"at\": 425}"),
        "{TimeOfDay("07:05") == TimeOfDay.parse("07:05")}",
    ]
}
