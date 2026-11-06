// Only injectables (services, protocols, repositories, configs, platform interfaces) can be injected.
struct Point {
    let x: Int
}

component Plot {
    @Inject let origin: Point

    view {
        text { "{origin.x}" }
    }
}
