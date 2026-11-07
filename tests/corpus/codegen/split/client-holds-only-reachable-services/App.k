import pay.Gateway
import pay.LocalGateway
import pay.RealGateway

public app Reach {
    use RealGateway for Gateway in .production
    use LocalGateway for Gateway in .development, .test
}
