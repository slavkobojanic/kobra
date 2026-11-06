// An import after a declaration is K0111, but it still binds its name: no K1202 at the use.
let count = 0
import k.time.Clock

service Stamps {
    @Inject let clock: Clock
}
