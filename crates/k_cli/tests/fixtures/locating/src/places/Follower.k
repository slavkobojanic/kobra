/// Follows the device while it is shown: an effect over
/// `Geolocation.watch`, which may start without a click once the
/// permission is granted.
public component Follower {
    @Inject let follow: FollowService

    @State var status: String = "idle"

    @Effect
    func following() async {
        status = "following"

        do {
            try await follow.run()
            status = "ended"
        } catch .notAsked {
            status = "not asked"
        } catch .denied {
            status = "denied"
        } catch .unsupported {
            status = "unsupported"
        } catch .unavailable, .timeout {
            status = "no fix"
        }
    }

    view {
        text { "Seen {follow.seen} positions" }
        text { "Last {follow.last}" }
        text { "Status {status}" }
    }
}
