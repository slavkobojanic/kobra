// `currentPosition` throws `LocationError`, whose five cases a `catch` must cover (45-web-platform.md,
// "Geolocation"); `watch` throws synchronously, so it is called with `try` and no `await`.
import k.device.Geolocation
import k.device.LocationAccuracy

service Locator {
    @Inject let geolocation: Geolocation

    func describe() async -> String {
        do {
            let p = try await geolocation.currentPosition(accuracy: .precise, maximumAge: 1.minute, timeout: 5.seconds)
            return "{p.latitude} ± {p.accuracyMeters} m at {p.measuredAt}"
        } catch .unsupported {
            return "unsupported"
        } catch .notAsked {
            return "not asked"
        } catch .denied {
            return "denied"
        } catch .unavailable {
            return "no fix"
        }
    }

    func start(_ accuracy: LocationAccuracy) {
        let positions = try geolocation.watch(accuracy: accuracy)
    }
}
