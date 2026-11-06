// Services that inject a client-only platform interface or service are client-only.
import k.storage.LocalStorage
import k.ui.Theme

service Drafts {
//      ^^^^^^ side: client-only
    @Inject let storage: LocalStorage
}

service Appearance {
//      ^^^^^^^^^^ side: client-only
    @Inject let theme: Theme
}

service Editor {
//      ^^^^^^ side: client-only
    @Inject let drafts: Drafts
}
