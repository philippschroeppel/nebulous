# Nebulous

A cross-cloud container orchestrator

Think of it as a Kubernetes that can span clouds with a focus on accelerated compute and AI workloads. Ships as a single binary, performant and lightweight via Rust.   
   
Why not Kubernetes? See [why_not_kube.md](docs/why_not_kube.md)   
   
Nebulous is in __alpha__, things may break.

## Installation

```sh
curl -fsSL -H "Cache-Control: no-cache" https://raw.githubusercontent.com/agentsea/nebulous/main/remote_install.sh | bash
```
* _Only MacOS and Linux arm64/amd64 are supported at this time._

## Usage

Run a local API server
```sh
nebu serve
```

Login to an API server
```sh
nebu login --url http://localhost:3000
```

Alternatively, login to our cloud
```sh
nebu login
```

### Containers

Create a container on runpod with 2 A100 GPUs which trains a model using TRL.
```yaml
kind: Container
metadata:
  name: trl-job
  namespace: training
  labels:
    type: llm-training
image: "huggingface/trl-latest-gpu:latest"
command: |
  source activate trl && trl sft --model_name_or_path $MODEL \
      --dataset_name $DATASET \
      --output_dir /output \
      --torch_dtype bfloat16 \
      --use_peft true
platform: runpod
env:
  - key: MODEL
    value: Qwen/Qwen2.5-7B 
  - key: DATASET
    value: trl-lib/Capybara 
volumes:  
  - source: /output
    dest: s3://my-bucket/training-output
    driver: RCLONE_SYNC
    continuous: true
accelerators:
  - "2:A100_SXM"
meters:
  - cost: 0.01
    unit: second
    metric: runtime
    currency: USD
restart: Never
```
Replace `my-bucket` with your bucket name, and make sure your aws and runpod credentials are in your environment.

```sh
nebu create container -f examples/containers/trl_small.yaml
```

Alternatively, create a container on EC2 with 1 L40s GPU
```sh
nebu create container \
    --name foo \
    --image tensorflow/tensorflow:latest \
    --cmd "echo hello" \
    --platform ec2 \
    --accelerators "1:L40s"
```

List all containers
```sh
nebu get containers
```

Get one container
```sh
nebu get containers trl-job -n training
```

Delete a container
```sh
nebu delete containers trl-job -n training
```

List available accelerators
```sh
nebu get accelerators
```

List available platforms
```sh
nebu get platforms
```

SSH into a container [in progress]
```sh
nebu ssh trl-job -n training
```

Exec a command in a container [in progress]
```sh
nebu exec trl-job -n training -- echo "hello"
```

Copy files to a container [in progress]
```sh
nebu cp /path/to/file trl-job:/path/to/file -n training
```

Send an http request to a container [in progress]
```text
curl http://<name>.<namespace>.<kind>.nebu:8000
```
* _Requires tailnet to be enabled_

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

#### Organizations

Nebulous is multi-tenant from the ground up. Here is an example of creating a container under the `Agentsea` organization.

```sh
nebu create container \
    --name "Agentsea/foo" \
    --image tensorflow/tensorflow:latest \
    --cmd "echo hello" \
    --platform ec2 \
    --accelerators "1:L40s"
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

---   

See [container examples](examples/containers) for more.

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

---

See [service examples](examples/services) for more.

### Processors [in progress]

Processors are containers that work off real-time data streams and are autoscaled based on back-pressure. Streams are provided by [Redis Streams](https://redis.io/docs/latest/develop/data-types/streams/).

Processors are best used for bursty async jobs, or low latency stream processing.

```yaml
kind: Processor
metadata:
  name: summarizer
  namespace: my-app
stream: my-app:workers:summarize
container:
  image: corge/summarizer:latest
  command: "redis-cli XREAD COUNT 10 STREAMS my-app:workers:summarize"
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
nebu create processor -f examples/processors/summarizer.yaml
```

Processors can also scale to zero.

```yaml
min_workers: 0
```

Processors can enforce schemas.

```yaml
schema:
  - name: text_to_summarize
    type: string
    required: true
```

Send data to a processor stream

```sh
nebu send processor summarizer --data '{"text_to_summarize": "Dlrow Olleh"} -n my-app'
```

Read data from a processor stream

```text
nebu read processor summarizer --num 10
```

List all processors

```sh
nebu get processors
```

Processors can use containers across different platforms. [in progress]

```yaml
container:
  image: corge/summarizer:latest
  command: "redis-cli XREAD COUNT 10 STREAMS my-app:workers:summarize"
  platforms:
    - gce
    - runpod
  accelerators:
    - "1:A40"
```

---   

See [processor examples](examples/processors) for more.

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
   
Processors also work with Clusters

```yaml
kind: Processor
stream: foo:bar:baz
cluster:
  container:
    image: quz/processor:latest
    command: "redis-cli XREAD COUNT 10 STREAMS foo:bar:baz"
    accelerators:
      - "8:H100"
    platform: ec2
  num_nodes: 4
min_workers: 1
max_workers: 10
```
---

See [cluster examples](examples/clusters) for more.

### Namespaces [in progress]

Namespaces provide a means to segregate groups of resources across clouds.  

```yaml
kind: Container
metadata:
  name: llama-factory-server
  namespace: my-app
```
   
Resources within a given namespace are network isolated using [Tailnet](https://tailscale.com/kb/1136/tailnet), and can be accessed by simply using `http://<name>.<namespace>.<kind>.nebu` e.g. `http://vllm-server.my-app.container.nebu`.
    
Nebulous cloud provides a free hosted [HeadScale](https://github.com/juanfont/headscale) instance to connect your resources, or you can bring your own by simply setting the `NEBU_HEADSCALE_URL` environment variable.   

### Secrets [in progress]

Secrets are used to store sensitive information such as API keys and credentials.

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

## Contributing

Please open an issue or submit a PR.

## Developing

Add all the environment variables shown in the [.env_](.env_) file to your environment.

Run a postgres and redis instance locally. This can be done easily with docker.

```sh
docker run -d --name redis -p 6379:6379 redis:latest
docker run -d --name postgres -p 5432:5432 postgres:latest
```
   
To use OpenMeter for metered billing, you will need to open an account with either [their cloud](https://openmeter.cloud/) or run their [open source](https://github.com/openmeterio/openmeter) and set the `OPENMETER_API_KEY` and `OPENMETER_URL` environment variables.
   
To use Tailnet, you will need to open an account with [Tailscale](https://tailscale.com/) or run your own [HeadScale](https://github.com/juanfont/headscale) instance and set the `TAILSCALE_API_KEY` and `TAILSCALE_TAILNET` environment variables.
   
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

When you make changes, simple run `make install` and `nebu serve` again.

## Inspiration

- [Kubernetes](https://kubernetes.io/)
- [Aurea](https://github.com/aurae-runtime/aurae)
- [RunPod](https://runpod.io/)
- [Prime Intellect](https://primeintellect.com/)
