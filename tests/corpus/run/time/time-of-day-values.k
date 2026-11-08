// A TimeOfDay is a value outside job schedules too (35-time.md): its hour and minute, equal by
// time, usable as a set element and a map key, printed and encoded as its `HH:MM` text.
import k.observe.Log
import k.time.TimeOfDay

struct Opening implements Codable {
    let day: String
    let opens: TimeOfDay
    let closes: TimeOfDay?
}

func main() -> List<Opening> {
    let nightly = TimeOfDay("03:00")
    let late = TimeOfDay("23:59")
    let times: Set<TimeOfDay> = [nightly, TimeOfDay("03:00"), late]
    let names: Map<TimeOfDay, String> = [nightly: "backup", late: "report"]
    Log.info("{nightly.hour} {nightly.minute} {late.hour} {late.minute}")
    Log.info("{nightly == TimeOfDay("03:00")} {nightly == late} {times.count} {names[late] ?? "none"}")
    Log.info("the backup runs at {nightly}")
    return [
        Opening(day: "monday", opens: TimeOfDay("09:30"), closes: TimeOfDay("17:00")),
        Opening(day: "sunday", opens: TimeOfDay("00:00"), closes: nil),
    ]
}
