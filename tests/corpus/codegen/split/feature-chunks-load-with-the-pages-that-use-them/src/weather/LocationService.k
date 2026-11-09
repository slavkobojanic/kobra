import k.device.Geolocation

public service LocationService {
    @Inject let geolocation: Geolocation

    @Derived let canLocate: Bool = geolocation.isSupported

    func locate() async -> Double {
        do {
            let position = try await geolocation.currentPosition()
            return position.latitude
        } catch _ {
            return 0.0
        }
    }
}
