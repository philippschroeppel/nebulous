<p align="center">
  <img src="./static/nebu_logo1_alpha.png" alt="Nebulous Logo" width="400">
</p>


__A globally distributed container orchestrator__

Think of it as a Kubernetes that can span clouds and regions with a focus on __accelerated compute__. Ships as a single binary, performant and lightweight via Rust.   
   
Why not Kubernetes? See [why_not_kube.md](docs/why_not_kube.md)   
   
:warning: Nebulous is in __alpha__, things may break.

## Installation

```sh
curl -fsSL -H "Cache-Control: no-cache" https://raw.githubusercontent.com/agentsea/nebulous/main/remote_install.sh | bash
```
> [!NOTE]
> Only MacOS and Linux arm64/amd64 are supported at this time.

## Usage   

Export the keys of your cloud providers.
```sh
export RUNPOD_API_KEY=...
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
```

Run a local API server on docker
```sh
nebu serve --docker
```

Or optionally run on Kubernetes with our [helm chart](./deploy/charts/nebulous/)   
   
Connect to the tailnet
```sh
nebu connect
```
    
See what cloud platforms are currently supported.
```sh
nebu get platforms
```

> [!TIP]
> Prefer a pythonic interface? Try [nebulous-py](https://github.com/agentsea/nebulous-py)

### Containers

Let's run our first container. We'll create a container on runpod with 2 A100 GPUs which trains a model using TRL.   
   
First, let's find what accelerators are available.
```sh
nebu get accelerators
```

Now lets create a container.
```yaml
kind: Container
metadata:
  name: trl-job
  namespace: training
image: "huggingface/trl-latest-gpu:latest"
platform: runpod
command: |
  source activate trl && trl sft --model_name_or_path $MODEL \
      --dataset_name $DATASET \
      --output_dir /output \
      --torch_dtype bfloat16 \
      --use_peft true
env:
  - key: MODEL
    value: Qwen/Qwen2.5-7B 
  - key: DATASET
    value: trl-lib/Capybara 
volumes:  
  - source: /output
    dest: s3://<my-bucket>/training-output
    driver: RCLONE_COPY
    continuous: true
accelerators:
  - "2:A100_SXM"
restart: Never
```
Replace `<my-bucket>` with a bucket name your aws credentials have access to, and edit any other fields as needed.

```sh
nebu create container -f mycontainer.yaml
```
> [!TIP]
> See our [container examples](examples/containers) for more.

List all containers
```sh
nebu get containers
```

Get the container we just created.
```sh
nebu get containers trl-job -n training
```

Exec a command in a container
```text
nebu exec trl-job -n training -c "echo hello"
```

Get logs from a container
```sh
nebu logs trl-job -n training
```

Send an http request to a container
```sh
curl http://container-{id}:8000
```

#### Queues

Containers can be assigned to a FIFO queue, which will block them from starting until the queue is free.

```yaml
kind: Container
image: pytorch/pytorch:latest
queue: actor-critic-training
```

#### Volumes

Volumes provide a means to persist and sync data accross clouds. Nebulous uses [rclone](https://rclone.org/) to sync data between clouds backed by an object storage provider.

```yaml
volumes:
  - source: s3://nebulous-rs/test
    dest: /test
    driver: RCLONE_SYNC
    continuous: true
```

Supported drivers are:
- `RCLONE_SYNC`
- `RCLONE_COPY`
- `RCLONE_BISYNC`

#### Organizations

Nebulous is multi-tenant from the ground up. Here is an example of creating a container under the `agentsea` organization.

```sh
nebu create container \
    --name "foo" \
    --owner "agentsea" \
    --image tensorflow/tensorflow:latest \
    --cmd "echo hello" \
    --platform ec2 \
    --accelerators "1:L40s"
```

The authorization heirarchy is
```
orgs -> namespaces -> resources
```

#### Meters

Metered billing is supported through [OpenMeter](https://openmeter.cloud/) using the `meters` field.

```yaml
meters:
  - cost: 0.1
    unit: second
    currency: USD
    metric: runtime 
```

Cost plus is supported through the `costp` field.

```yaml
meters:
  - costp: 10
    unit: second
    currency: USD
    metric: runtime 
```
This configuration will add 10% to the cost of the container.

#### Authz

Authz is supported through the container proxy. 

To enable the proxy for a container, set the `proxy_port` field to the container port you want to proxy.
```yaml
proxy_port: 8080
```

Then your service can be accesssed at `http://proxy.<nebu-host>` with the header `x-resource: <name>.<namespace>.<kind>`.   

With the proxy enabled, you can also configure authz rules.

```yaml
authz:
  rules:
    # Match on email
    - name: email-match
      field_match:
        - field: "owner"
          pattern: "${email}"
      allow: true
    
    # Path-based matching for organization resources
    - name: org-path-access
      path_match:
        - pattern: "/api/v1/orgs/${org_id}/**"
        - pattern: "/api/v1/organizations/${org_id}/**"
        - pattern: "/api/v1/models/${org_id}/**"
      allow: true
```

Variables are interpolated from the users auth profile.

> [!TIP]
> See [container examples](examples/containers) for more.

### Secrets

Secrets are used to store sensitive information such as API keys and credentials. Secrets are `AES-256` encrypted and stored in the database.

Create a secret
```sh
nebu create secret my-secret --value $MY_SECRET_VALUE -n my-app
```

Get all secrets
```sh
nebu get secrets -n my-app
```

Get a secret
```sh
nebu get secrets my-secret -n my-app
```

Delete a secret
```sh
nebu delete secrets my-secret -n my-app
```

Secrets can be used in container environment variables.

```yaml
kind: Container
metadata:
  name: my-container
  namespace: my-app
env:
  - key: MY_SECRET
    secret_name: my-secret
```

### Namespaces

Namespaces provide a means to segment groups of resources across clouds.  

```yaml
kind: Container
metadata:
  name: llama-factory-server
  namespace: my-app
```
   
Resources within a given namespace are network isolated using [Tailnet](https://tailscale.com/kb/1136/tailnet), and can be accessed by simply using `http://{kind}-{id}` e.g. `http://container-12345:8000`.
    
Nebulous cloud provides a free hosted [HeadScale](https://github.com/juanfont/headscale) instance to connect your resources, or you can bring your own by simply setting the `TAILSCALE_URL` environment variable.   

### Services [in progress]

Services provide a means to expose containers on a stable IP address, and to balance traffic across multiple containers. Services auto-scale up and down as needed.

```yaml
kind: Service
metadata:
  name: vllm-qwen
  namespace: inference
container:
  image: vllm/vllm-openai:latest
  command: |
    python -m vllm.entrypoints.api_server \
      --model Qwen/Qwen2-7B-Instruct \
      --tensor-parallel-size 1 \
      --port 8000
  accelerators:
    - "1:A100"
platform: gce
min_containers: 1
max_containers: 5
scale:
  up:
    above_latency: 100ms
    duration: 10s
  down:
    below_latency: 10ms
    duration: 5m
  zero:
    below_latency: 10ms
    duration: 10m
```

```sh
nebu create service -f examples/service/vllm-qwen.yaml
```

The IP will be returned in the `status` field.

```sh
nebu get services vllm-qwen -n inference
```

Service can be buffered, which will queue requests until a container is available.

```yaml
buffered: true
```

Services can also scale to zero.

```yaml
min_containers: 0
```

Services can also enforce schemas.

```yaml
schema:
  - name: prompt
    type: string
    required: true
```

Or use a common schema.

```yaml
common_schema: OPENAI_CHAT
```

Services can record all requests and responses.

```yaml
record: true
```

Services can perform metered billing, such as counting the number of tokens in the response.

```yaml
meters:
  - cost: 0.001
    unit: token
    currency: USD
    response_json_value: "$.usage.prompt_tokens"
```

Services also work with clusters.

```yaml
kind: Service
metadata:
  name: vllm-qwen
  namespace: inference
cluster:
  container:
    image: vllm/vllm-openai:latest
    command: |
      python -m vllm.entrypoints.api_server \
        --model Qwen/Qwen2-72B-Instruct \
        --tensor-parallel-size 1 \
        --port 8000
    accelerators:
      - "8:A100"
  num_nodes: 2
```

> [!TIP]
> See [service examples](examples/services) for more.
   
### Clusters [in progress]

Clusters provide a means of multi-node training and inference.

```yaml
kind: Cluster
metadata:
  name: pytorch-test
  namespace: foo
container:
  image: pytorch/pytorch:latest
  command: "echo $NODES && torchrun ..."
  platform: ec2
  env:
    - key: HELLO
      value: world
  volumes:
    - source: s3://nebulous-rs/test
      dest: /test
      driver: RCLONE_SYNC
      continuous: true
  accelerators:
    - "8:B200"
num_nodes: 4
```
```sh
nebu create cluster -f examples/cluster.yaml
```

Each container will get a `$NODES` env var which contains the IP addresses of the nodes in the cluster.   
   
Clusters always aim to schedule nodes as close to each other as possible, with as fast of networking as available.   

> [!TIP]
> See [cluster examples](examples/clusters) for more.

### Processors [in progress]

Processors are containers that work off real-time data streams and are autoscaled based on back-pressure. Streams are provided by [Redis Streams](https://redis.io/docs/latest/develop/data-types/streams/).

Processors are best used for bursty async jobs, or low latency stream processing.

```yaml
kind: Processor
metadata:
  name: translator
  namespace: my-app
stream: my-app:workers:translator
container:
  image: corge/translator:latest
  command: "redis-cli XREAD COUNT 10 STREAMS my-app:workers:translator"
  platform: gce
  accelerators:
    - "1:A40"
min_workers: 1
max_workers: 10
scale:
  up:
    above_pressure: 100
    duration: 10s
  down:
    below_pressure: 10
    duration: 5m
  zero:
    duration: 10m
```
```sh
nebu create processor -f examples/processors/translator.yaml
```

Processors can also scale to zero.

```yaml
min_workers: 0
```

Processors can enforce schemas.

```yaml
schema:
  - name: text_to_translate
    type: string
    required: true
```

Send data to a processor stream

```sh
nebu send processor translator --data '{"text_to_translate": "Dlrow Olleh"} -n my-app'
```

Read data from a processor stream

```text
nebu read processor translator --num 10
```

List all processors

```sh
nebu get processors
```

Processors can use containers across different platforms. [in progress]

```yaml
container:
  image: corge/translator:latest
  command: "redis-cli XREAD COUNT 10 STREAMS my-app:workers:translator"
  platforms:
    - gce
    - runpod
  accelerators:
    - "1:A40"
```

> [!TIP]
> See [processor examples](examples/processors) for more.

## SDK

:snake: Python https://github.com/agentsea/nebulous-py    
   
:crab: Rust https://crates.io/crates/nebulous/versions

## Roadmap

- [ ] Services
- [ ] Clusters
- [ ] Processors
- [ ] Support for AWS EC2
- [ ] Support non-gpu containers
- [ ] Support for GCE
- [ ] Support for Azure
- [ ] Support for Kubernetes

## Contributing

Please open an issue or submit a PR.

## Developing

Add all the environment variables shown in the [.env_](.env_) file to your environment.

Run a postgres and redis instance locally. This can be done easily with docker.

```sh
docker run -d --name redis -p 6379:6379 redis:latest
docker run -d --name postgres -p 5432:5432 postgres:latest
```

To configure the secrets store you will need an encryption key. This can be generated with the following command.
```sh
openssl rand -base64 32 | tr -dc '[:alnum:]' | head -c 32
```
Then set this to the `NEBU_ENCRYPTION_KEY` environment variable.   
     
To optionally use OpenMeter for metered billing, you will need to open an account with either [their cloud](https://openmeter.cloud/) or run their [open source](https://github.com/openmeterio/openmeter) and set the `OPENMETER_API_KEY` and `OPENMETER_URL` environment variables.   
     
To optionally use Tailnet, you will need to open an account with [Tailscale](https://tailscale.com/) or run your own [HeadScale](https://github.com/juanfont/headscale) instance and set the `TAILSCALE_API_KEY` and `TAILSCALE_TAILNET` environment variables.
   
Install locally
```
make install
```
   
Run the server
```
nebu serve
```
   
Login to the auth server. When you do, set the server to `http://localhost:3000`.
```
nebu login
```
   
Now you can create resources

```sh
nebu create container -f examples/containers/trl_small.yaml
```
   
When you make changes, simply run `make install` and `nebu serve` again.

## Inspiration

- [Kubernetes](https://kubernetes.io/)
- [Aurea](https://github.com/aurae-runtime/aurae)
- [RunPod](https://runpod.io/)
- [Prime Intellect](https://primeintellect.com/)
