// Everything that is neither server-only nor client-only is shared.
import k.auth.Session
import k.http.HttpClient
import k.rpc.RpcOnlyError
import k.time.Clock

protocol WeatherProtocol {
//       ^^^^^^^^^^^^^^^ side: shared
    rpc today() throws RpcOnlyError -> Int
}

struct Forecast {
//     ^^^^^^^^ side: shared
    let degrees: Int
}

enum Unit {
//   ^^^^ side: shared
    case celsius
}

service Weather {
//      ^^^^^^^ side: shared
    @Inject let api: WeatherProtocol
    @Inject let http: HttpClient
    @Inject let clock: Clock
    @Inject let session: Session
}

component Badge {
//        ^^^^^ side: shared
    @Inject let weather: Weather

    view {
        text { "badge" }
    }
}

page Home {
//   ^^^^ side: shared
    view {
        Badge()
    }
}

func celsius(fahrenheit: Int) -> Int {
//   ^^^^^^^ side: shared
    return (fahrenheit - 32) * 5 / 9
}
