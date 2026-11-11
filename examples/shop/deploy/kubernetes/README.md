# K Shop on Kubernetes

The same system as `docker-compose.yml`, on a local [kind](https://kind.sigs.k8s.io) cluster: Postgres,
Mailpit, paybox and two shop pods, with no account anywhere. Every command runs at the workspace root,
`examples/shop`.

| File | What it holds |
|---|---|
| `kind-config.yaml` | a one-node cluster, with host port 8080 mapped to the ingress controller |
| `namespace.yaml` | the `k-shop` namespace |
| `config.yaml` | each app's configuration that isn't secret (ConfigMaps), and Postgres' init script |
| `postgres.yaml` | Postgres as a StatefulSet with a volume, and its headless Service `db` |
| `mailpit.yaml` | Mailpit (SMTP on 1025, the inbox on 8025) and its Service `mail` |
| `migrate.yaml` | one Job per app running `dist/app migrate` |
| `paybox.yaml` | paybox's Deployment (with startup, readiness and liveness probes on `/health`) and Service |
| `shop.yaml` | the shop's Deployment of two pods (same probes) and Service |
| `ingress.yaml` | the shop at `http://localhost:8080` through an ingress controller of class `nginx` |
| `create-secrets.sh` | the Secrets, from the files in `secrets/` |
| `kustomization.yaml` | everything but the Secrets, for `kubectl apply --kustomize` |

Secrets never appear in the manifests. `create-secrets.sh` makes three Secrets from the files
`scripts/init-secrets.sh` generates, and each pod mounts its Secret at `/run/secrets`, where the
`NAME_FILE` variables of `config.yaml` point, exactly as with Docker Compose. In the K repository,
`cargo test -p k_cli --test it -- deploy::` checks that these manifests give every container what its
`dist/app config` lists, and `K_KIND=1 cargo test -p k_cli --test it -- kubernetes:: --test-threads 1`
runs this walkthrough on a throwaway kind cluster.

## Walkthrough

You need [Docker](https://docs.docker.com/get-docker/), [kind](https://kind.sigs.k8s.io/docs/user/quick-start/#installation)
and [kubectl](https://kubernetes.io/docs/tasks/tools/).

**1. Build the images and generate the secrets.** The images are the ones Docker Compose builds, from the
same Dockerfile:

```bash
scripts/init-secrets.sh
docker compose build paybox shop          # k-paybox:local and k-shop:local
```

**2. Create the cluster and load the images into it.** kind runs its own container runtime, so local
images are loaded rather than pulled from a registry; the manifests use `imagePullPolicy: IfNotPresent`.

```bash
kind create cluster --name k-shop --config deploy/kubernetes/kind-config.yaml
kind load docker-image k-paybox:local k-shop:local --name k-shop
```

**3. Optionally, install an ingress controller** that serves the `nginx` ingress class, as
[kind's ingress guide](https://kind.sigs.k8s.io/docs/user/ingress/) describes; `kind-config.yaml`
already labels the node and maps host port 8080 to its port 80. Without one, step 6 forwards a port
instead.

**4. Create the Secrets and apply the manifests.**

```bash
deploy/kubernetes/create-secrets.sh
kubectl apply --kustomize deploy/kubernetes
```

Postgres starts and creates paybox's database on its new volume. The two migration Jobs retry until it
answers, apply each app's migrations and complete; the Deployments' `serve` would apply them too (under
an advisory lock), so the order doesn't matter. Wait for everything:

```bash
kubectl --namespace k-shop wait --for=condition=complete job --all --timeout=300s
kubectl --namespace k-shop rollout status deployment/paybox deployment/shop --timeout=300s
```

**5. Load the demo catalog.**

```bash
kubectl --namespace k-shop exec --stdin statefulset/db -- \
    psql --username=shop --dbname=shop < shop/scripts/demo-catalog.sql
```

**6. Open the shop** at http://localhost:8080 and pay with `4242 4242 4242 4242`. Without an ingress
controller, forward the port first; the origin is the same, so `K_PUBLIC_ORIGIN` still matches:

```bash
kubectl --namespace k-shop port-forward service/shop 8080:8080
```

The confirmation email is in Mailpit:

```bash
kubectl --namespace k-shop port-forward service/mail 8025:8025      # then http://localhost:8025
```

Sign in at http://localhost:8080/sign-in as `admin@shop.example` with the password in
`secrets/admin_password.txt`.

## Operating it

| Task | Command |
|---|---|
| Logs of both shop pods | `kubectl --namespace k-shop logs deployment/shop --all-containers --prefix` |
| What a build reads | `kubectl --namespace k-shop exec deployment/shop -- /app config` |
| Payment events paybox couldn't deliver | `kubectl --namespace k-shop exec deployment/paybox -- /app jobs list --dead` |
| Retry one | `kubectl --namespace k-shop exec deployment/paybox -- /app jobs retry <id>` |
| A new release | rebuild, `kind load docker-image …`, delete the migration Jobs, `kubectl apply --kustomize deploy/kubernetes`, `kubectl --namespace k-shop rollout restart deployment/paybox deployment/shop` |
| Remove everything | `kind delete cluster --name k-shop` |

A rolling restart drops no request. Each app pod first sleeps 5 seconds (`preStop`), while every node
stops routing new connections to it, then drains for up to `K_SHUTDOWN_GRACE` (25 s) after `SIGTERM`,
within its 35-second termination grace period. Without the sleep, connections that kube-proxy still
sent to a pod that had stopped accepting were refused. A shop pod that stops answering `/health` (its
database is unreachable) leaves the Service until it recovers.

No pod mounts a Kubernetes API token (`automountServiceAccountToken: false`): nothing calls the API, and
the token's mount point, `/var/run/secrets/kubernetes.io`, would have to be created inside the read-only
Secret volume at `/run/secrets`, which keeps the container from starting.

## Beyond a local cluster

- Push the images to your registry and drop `imagePullPolicy: IfNotPresent`.
- Set `K_PUBLIC_ORIGIN` in `config.yaml` to the public `https://` origin, terminate TLS at the ingress
  (HTTP/2 there, for server streams), and set `K_TRUSTED_PROXIES` to the ingress controller's range.
- Point `SMTP_*` at a real mail server, and use a managed Postgres instead of the StatefulSet.
- Create the Secrets from your secret store instead of `create-secrets.sh`, keeping the same keys.
