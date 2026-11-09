import k.device.Geolocation

public service LocationService {
    @Inject let geolocation: Geolocation

    @Derived let canLocate: Bool = geolocation.isSupported and geolocation.permission != .denied

    @State var latitude: Double = 0.0

    func locate() async {
        if geolocation.permission == .prompt {
            let answer = await geolocation.requestAccess()
        }
        do {
            let position = try await geolocation.currentPosition()
            latitude = position.latitude
        } catch _ {
        }
    }
}
