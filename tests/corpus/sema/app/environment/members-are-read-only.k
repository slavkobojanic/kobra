// The environment follows the browser: a view can't write it.
import k.device.Connectivity

component Toggle {
    @Inject let connectivity: Connectivity

    view {
        button { "Pretend" }
            .on(click: { connectivity.isOnline = false })
    }
}
