// The generated `copy` and derived `==` are usable from another module.
import accounts.User

func rename(user: User) -> Bool {
    let renamed = user.copy(name: "Ana")
      //^^^^^^^ type: accounts.User
    return renamed == user
}
