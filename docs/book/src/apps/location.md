# The user's location

Where the device is comes from the client-only **`Geolocation`** interface in `k.device`. Like
`Files` and `Clipboard`, a service injects it, so its errors are handled in one place and a test
replaces it. The design is in `docs/design/45-web-platform.md`.

```k
// examples/weather/src/weather/LocationService.k
public service LocationService {
    @Inject let geolocation: Geolocation

    /// Hidden when the browser can't locate, or the user has blocked it.
    /// Reactive: it follows the site's settings.
    @Derived let canLocate: Bool = geolocation.isSupported and geolocation.permission != .denied
// ...
    func locate() async throws LocateError -> Position {
        if geolocation.permission == .prompt {
            let answer = await geolocation.requestAccess()

            guard answer == .granted else { throw LocateError.denied }
        }

        do {
            return try await geolocation.currentPosition(accuracy: .coarse)
        } catch .unavailable {
            throw LocateError.noFix
// ...
        }
    }
}
```

Each case of `LocationError` gets its own `catch`, so the compiler checks that none is forgotten.

- **Only `requestAccess()` shows the browser's prompt.** It is `@UserAction`: call it from a click,
  submit or key handler, before the handler's first `await`, or the compiler reports K4030. A prompt
  nobody asked for is what browsers block and users dismiss. It returns the new `PermissionState`; a
  refusal is an answer, not an error.
- **Every other member checks the permission and never prompts.** `currentPosition` and `watch`
  throw `.notAsked` before the user was asked and `.denied` after a refusal, so a page that already
  holds a grant can read the position in an effect.
- **`permission` and `isSupported` are reactive.** A view or a `@Derived` value that reads them
  updates when the user changes the site's settings. While the server renders, `isSupported` is
  `false` and `permission` is `.prompt`, so a "Use my location" button built on them appears after
  hydration, in browsers that can locate. Any other member read where the server renders is K4012.
- `currentPosition(accuracy:maximumAge:timeout:)` reads one `Position`; `.coarse`, the default, is
  enough for a city and lighter on the battery. It throws a `LocationError`: `.unsupported`,
  `.notAsked`, `.denied`, `.unavailable` (no fix) or `.timeout`.
- `watch(accuracy:)` returns a `Stream<Position>` that ends when the permission is revoked or its
  owner is cancelled, which stops the browser's watch.
- A `Position` holds plain numbers with their unit in the name: `latitude`, `longitude`,
  `accuracyMeters`, the optional `altitudeMeters`, `headingDegrees` and `speedMetersPerSecond`, and
  `measuredAt`.

## Testing

`k test` puts **`TestGeolocation`** in place of the browser's: no prompt opens and the machine's
position is never read. The browser can locate, the permission starts at `.prompt`, prompts are
answered `.granted`, and the device has no fix until the test moves it:

```k
// examples/weather/src/components/UseMyLocation.test.k
geolocation.move(to: Position.sample(latitude: 52.52, longitude: 13.41))
let screen = render(UseMyLocation())

screen.button(named: "Use my location").click()
await screen.settle()

expect geolocation.prompts == 1
expect geolocation.lastAccuracy == .coarse
expect screen.currentPage == .localWeather(latitude: "52.52", longitude: "13.41")
```

`answerPrompts(with:)` sets how the user answers, `setPermission(_:)` changes the site's settings
(anything but `.granted` ends open watches), `failNext(error:)` fails the next read, and
`setSupported(_:)` makes every call throw `.unsupported`. `prompts` and `lastAccuracy` record what
was asked. `Position.sample(latitude:longitude:)` is a position with 1 km accuracy, measured at the
`TestClock`'s time.
