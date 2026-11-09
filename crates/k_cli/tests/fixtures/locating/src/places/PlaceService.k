/// Where the user chose to look.
public service PlaceService {
    @State var latitude: Double? = nil
    @State var longitude: Double? = nil

    func select(latitude: Double, longitude: Double) {
        self.latitude = latitude
        self.longitude = longitude
    }
}
