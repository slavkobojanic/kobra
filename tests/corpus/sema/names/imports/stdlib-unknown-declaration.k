// A misspelled stdlib import is K1101 with the closest name; the field typed with it reports nothing more.
import k.time.Clok

service Stamps {
    @Inject let clock: Clok
}
