// The environment services are `.app` scoped, so a `.server` service can't
// inject them (45-web-platform.md, "Server rendering and the server").
import k.device.Connectivity

@Scope(.server)
service Warmup {
    @Inject let connectivity: Connectivity
}
