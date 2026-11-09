import k.device.PageVisibility
import k.time.Clock

/// Refreshes every minute while the page is visible: an effect that reads
/// `isVisible` starts and stops its timer with the page.
public component Refresher {
    @Inject let visibility: PageVisibility
    @Inject let clock: Clock

    @State var refreshes: Int = 0

    @Effect
    func refreshWhileVisible() {
        guard visibility.isVisible else { return }

        let timer = clock.every(1.minute) { refreshes += 1 }

        onCleanup { timer.cancel() }
    }

    view {
        text { "Refreshed {refreshes} times" }
        if not visibility.isVisible {
            text { "Paused" }
        }
    }
}
