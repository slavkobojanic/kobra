import k.test.MemoryStorage

test RecentPlaces {
    @Inject let storage: MemoryStorage
    @Inject let recent: RecentPlaces

    test "with nothing saved, the list starts empty" {
        expect recent.places.isEmpty
    }

    test "the newest place comes first, and a place is listed once" {
        recent.remember(place: .berlin)
        recent.remember(place: .lisbon)
        recent.remember(place: .berlin)

        expect recent.places.map { place => place.id } == [Place.berlin.id, Place.lisbon.id]
        expect recent.places[0].name == "Berlin, Land Berlin, Germany"
    }

    test "only the most recent places are kept" {
        for id in [1, 2, 3, 4, 5] {
            recent.remember(place: Place(id: id, name: "P{id}", region: nil, country: nil, latitude: 0, longitude: 0))
        }

        expect recent.places.map { place => place.id } == [5, 4, 3, 2]
    }
}
