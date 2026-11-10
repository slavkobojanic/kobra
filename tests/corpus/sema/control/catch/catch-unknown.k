// `catch unknown` follows clauses naming every known case: it runs only
// for a case added later, so it is never unreachable. Once a case is
// added, it still catches it, and K2207 (a warning) lists what the
// clauses before it no longer name.
enum SaveError implements Error {
    case offline
    case conflict
    case tooLarge
}

func save() throws SaveError {}

func everyCase() -> String {
    do {
        try save()
        return "saved"
    } catch .offline {
        return "offline"
    } catch .conflict {
        return "conflict"
    } catch .tooLarge {
        return "too large"
    } catch unknown {
        return "failed"
    }
}

func aCaseWasAdded() -> String {
    do {
        try save()
        return "saved"
    } catch .offline {
        return "offline"
    } catch .conflict {
        return "conflict"
    } catch unknown {
        return "failed"
    }
}

func afterACatchAll() {
    do {
        try save()
    } catch _ {
    } catch unknown {
    }
}

func nothingThrown() {
    do {
        log()
    } catch unknown {
    }
}

func log() {}
