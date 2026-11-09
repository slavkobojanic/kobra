import k.device.Geolocation
import k.device.LocationError

/// Watches the device for [`Follower`].
public service FollowService {
    @Inject let geolocation: Geolocation

    @State var seen: Int = 0
    @State var last: String = "none"

    /// Counts the positions until the watch ends.
    func run() async throws LocationError {
        let positions = try geolocation.watch(accuracy: .precise)

        for await position in positions {
            seen += 1
            last = "{position.latitude}"
        }
    }
}
