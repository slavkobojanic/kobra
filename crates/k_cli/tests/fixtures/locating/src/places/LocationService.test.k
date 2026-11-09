import k.device.LocationError
import k.device.Position
import k.test.TestClock
import k.test.TestGeolocation

test LocationService {
    @Inject let geolocation: TestGeolocation
    @Inject let service: LocationService
    @Inject let clock: TestClock

    test "the double starts capable, at prompt, with no fix" {
        expect geolocation.isSupported
        expect geolocation.permission == .prompt
        expect geolocation.prompts == 0
        expect geolocation.lastAccuracy == nil
        expect service.canLocate
    }

    test "reading before asking is not asked, and never prompts" {
        geolocation.move(to: Position.sample(latitude: 1.0, longitude: 2.0))

        expect await service.preciseNow() == "not asked"
        expect geolocation.prompts == 0
    }

    test "a granted permission reads the position with the accuracy asked for" {
        geolocation.setPermission(.granted)
        geolocation.move(to: Position.sample(latitude: 48.85, longitude: 2.35))

        expect await service.preciseNow() == "48.85,2.35"
        expect geolocation.lastAccuracy == .precise
    }

    test "a denied permission is denied" {
        geolocation.setPermission(.denied)

        expect await service.preciseNow() == "denied"
        expect not service.canLocate
    }

    test "the next read can fail, once" {
        geolocation.setPermission(.granted)
        geolocation.move(to: Position.sample(latitude: 1.0, longitude: 2.0))
        geolocation.failNext(error: .timeout)

        expect await service.preciseNow() == "timeout"
        expect await service.preciseNow() == "1.0,2.0"
    }

    test "without geolocation every read is unsupported, and nothing asks" {
        geolocation.setSupported(false)

        expect not geolocation.isSupported
        expect await service.preciseNow() == "unsupported"
        expect await geolocation.requestAccess() == .prompt
        expect geolocation.prompts == 0
    }

    test "requestAccess asks once, and answers the permission after" {
        expect await geolocation.requestAccess() == .granted
        expect await geolocation.requestAccess() == .granted
        expect geolocation.prompts == 1
    }

    test "a refused prompt throws denied from locate" {
        geolocation.answerPrompts(with: .denied)

        expect throws LocateError.denied {
            try await service.locate()
        }
        expect geolocation.permission == .denied
    }

    test "a sample position has 1 km accuracy and the test clock's time" {
        clock.advance(by: 90.seconds)
        let position = Position.sample(latitude: 52.52, longitude: 13.40)

        expect position.accuracyMeters == 1000.0
        expect position.altitudeMeters == nil
        expect position.headingDegrees == nil
        expect position.speedMetersPerSecond == nil
        expect position.measuredAt == clock.now
    }

    test "positions are values" {
        let a = Position.sample(latitude: 1.0, longitude: 2.0)

        expect a == Position.sample(latitude: 1.0, longitude: 2.0)
        expect a != Position.sample(latitude: 1.0, longitude: 3.0)
    }
}
