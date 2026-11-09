// Only `Geolocation.requestAccess()` prompts, and it is `@UserAction` (45-web-platform.md,
// "Geolocation"): asking from an effect is K4030, through the service that asks too. Reading the
// position or watching it never prompts, so an effect may do both once access is granted.
import k.device.Geolocation
import k.device.LocationError
import k.device.Position

service Locator {
    @Inject let geolocation: Geolocation

    @State var latitude: Double = 0.0

    func ask() async -> Bool {
        return await geolocation.requestAccess() == .granted
    }

    func read() async throws LocationError -> Position {
        return try await geolocation.currentPosition()
    }

    func follow() async throws LocationError {
        for await position in try geolocation.watch(accuracy: .precise) {
            latitude = position.latitude
        }
    }
}

component Map {
    @Inject let locator: Locator

    @Effect
    func askAtOnce() async {
        let granted = await locator.ask()
    }

    @Effect
    func followWhenGranted() async {
        do {
            try await locator.follow()
            let here = try await locator.read()
        } catch _ {
        }
    }

    view {
        button { "Locate me" }
            .on(click: { await locator.ask() })
    }
}
