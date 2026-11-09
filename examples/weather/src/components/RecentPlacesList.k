import weather.RecentPlaces

/// Links to the places the user opened recently, from local storage.
///
/// The list lives only in the browser, so this component is
/// `@Rendering(.client)`: the server renders its empty `loading` block in the
/// otherwise server-rendered start page, and the browser swaps in the list
/// right after hydration, once `RecentPlaces` has read local storage. It is
/// the last thing on the page, so nothing the server rendered moves when it
/// appears.
@Rendering(.client)
public component RecentPlacesList {
    @Inject let recent: RecentPlaces

    view {
        if not recent.places.isEmpty {
            section {
                heading(level: 2) { "Recently viewed" }

                list {
                    for place in recent.places key place.id {
                        item {
                            link(to: .cityWeather(placeId: place.id)) { place.name }
                                .class(.place)
                        }
                    }
                }
                .class(.places)
            }
        }
    }

    loading {
    }

    style {
        .places {
            display: flex;
            flexWrap: wrap;
            gap: .sm;
            listStyle: none;
            padding: 0;
        }

        .place {
            display: inlineBlock;
            padding: .sm .md;
            borderRadius: .md;
            color: .text;
            textDecoration: none;

            &:hover, &:focus-visible { color: .primary; }
        }
    }
}
