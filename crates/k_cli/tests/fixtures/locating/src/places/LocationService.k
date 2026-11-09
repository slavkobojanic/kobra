import k.device.Geolocation
import k.device.Position

/// The "Use my location" button's service, as
/// `docs/design/45-web-platform.md` writes it.
public service LocationService {
    @Inject let geolocation: Geolocation

    /// Hidden when the browser can't locate, or the user has blocked it.
    @Derived let canLocate: Bool = geolocation.isSupported and geolocation.permission != .denied

    /// Asks for access the first time, then reads one coarse position.
    func locate() async throws LocateError -> Position {
        if geolocation.permission == .prompt {
            let answer = await geolocation.requestAccess()

            guard answer == .granted else { throw LocateError.denied }
        }

        do {
            return try await geolocation.currentPosition(accuracy: .coarse)
        } catch .unavailable {
            throw LocateError.noFix
        } catch .timeout {
            throw LocateError.noFix
        } catch .denied {
            throw LocateError.denied
        } catch .notAsked {
            throw LocateError.denied
        } catch .unsupported {
            throw LocateError.unsupported
        }
    }

    /// Reads one precise position without asking, as code that runs
    /// without a click must.
    func preciseNow() async -> String {
        do {
            let position = try await geolocation.currentPosition(accuracy: .precise, maximumAge: 1.minute, timeout: 5.seconds)
            return "{position.latitude},{position.longitude}"
        } catch .unsupported {
            return "unsupported"
        } catch .notAsked {
            return "not asked"
        } catch .denied {
            return "denied"
        } catch .unavailable {
            return "unavailable"
        } catch .timeout {
            return "timeout"
        }
    }
}
