import k.device.Geolocation
import k.device.Position

/// The device's position, for the start page's "Use my location" button.
///
/// `Geolocation` is a client-only platform interface, so this service is
/// client-only too. Two of its members may still be read while the server
/// renders the start page: `isSupported` is `false` there and `permission`
/// is `.prompt`, so `canLocate` is `false` on the server and the button
/// appears after hydration, in browsers that can locate.
///
/// Only `requestAccess()` shows the browser's prompt, and the compiler
/// allows it only from a user's action: `locate()` asks from the button's
/// click, never from an effect or a load.
public service LocationService {
    @Inject let geolocation: Geolocation

    /// Hidden when the browser can't locate, or the user has blocked it.
    /// Reactive: it follows the site's settings.
    @Derived let canLocate: Bool = geolocation.isSupported and geolocation.permission != .denied

    /// Asks for access the first time, then reads one coarse position: a
    /// city's weather doesn't need the doorstep.
    ///
    /// - Throws: `LocateError.denied` when the user refuses,
    ///   `.unsupported` where the browser can't locate, and `.noFix` when
    ///   the device has no position or none came in time.
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
            // The permission was reset between the check and the read.
            throw LocateError.denied
        } catch .unsupported {
            throw LocateError.unsupported
        }
    }
}
