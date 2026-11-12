@Deprecated("use B")   public   struct A<T>implements Codable where T implements Codable{
    @Default(1)static let shared:Int=1
    var computed:Int{ shared*2 }
    func first<U>(of items:List<U>)async throws E->U? where U implements Codable
}

enum Tone{
    case neutral
    case warning(level:Int,label:String="x")
}

service S implements P {
    rpc load(id:Int)async throws E->Int requires Authenticated,RateLimit(100.perMinute){ id }
}
@SignInPage(.signIn)
public app Shelf {
    @NotBlank(message: "Enter the title")
    @Length(max: 200)
    var title: String
}
let a = 1
let b = 2
func g() {}
