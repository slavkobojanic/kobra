// A client-only interface can't be called from code that may run during server rendering, such as a
// field initializer; `init()` runs only in the browser, so the same call is fine there.
import k.storage.LocalStorage
import k.storage.StorageKey

let draftKey: StorageKey<String> = StorageKey(name: "draft")

service Drafts {
    @Inject let storage: LocalStorage

    let cleared: Void = storage.remove(key: draftKey)

    init() {
        storage.remove(key: draftKey)
    }
}
