// An import nothing in the file uses is a K1105 warning whose fix removes the whole line.
import k.http.HttpStatus
import k.time.Clock
import k.ui.Router

service Stamps {
    @Inject let clock: Clock
}

func ok() -> HttpStatus {
    return .ok
}
