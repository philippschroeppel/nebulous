# Nebulous

A cross-cloud container orchestrator

Think of it as a Kubernetes that can span clouds with a focus on accelerated compute and AI workloads. Ships as a single binary, performant and lightweight via Rust.

## Installation

```sh
curl -fsSL -H "Cache-Control: no-cache" https://raw.githubusercontent.com/agentsea/nebulous/main/remote_install.sh | bash
```

## Usage

Login to an API server
```sh
nebu login
```

### Containers

Create a container on runpod with 4 A100 GPUs
```yaml
kind: Container
metadata:
  name: pytorch-test
  namespace: foo
image: pytorch/pytorch:latest
command: nvidia-smi
platform: runpod
env_vars:
  - key: HELLO
    value: world
volumes:
  - source: s3://nebulous-rs/test
    dest: /nebu/test
    driver: RCLONE_SYNC
    continuous: true
accelerators:
  - "4:A100"
```
```sh
nebu create container -f examples/basic.yaml
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
nebu get containers foo
```

Delete a container
```sh
nebu delete containers foo
```

List available accelerators
```sh
nebu get accelerators
```

List available platforms
```sh
nebu get platforms
```

Get the IP address of a container [in progress]
```sh
nebu get containers foo --ip
```

SSH into a container [in progress]
```sh
nebu ssh foo
```

#### Queues

Containers can be assigned to a FIFO queue, which will block them from starting until the queue is free.

```yaml
kind: Container
image: pytorch/pytorch:latest
queue: actor-critic-training
...
```

#### Volumes

Volumes provide a means to persist and sync data accross clouds. Nebulous uses [rclone](https://rclone.org/) to sync data between clouds backed by an object storage provider.

```yaml
volumes:
  - source: s3://nebulous-rs/test
    dest: /nebu/test
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

### Processors

Processors are containers that work off real-time data streams and are autoscaled based on back-pressure. Streams are provided by [Redis Streams](https://redis.io/docs/latest/develop/data-types/streams/).

```yaml
kind: Processor
metadata:
  name: vllm-llama3
  namespace: inference
stream: inference:vllm:llama3
container:
  image: corge/vllm-processor:latest
  command: "redis-cli XREAD COUNT 10 STREAMS inference:vllm:llama3"
  platform: gce
  accelerators:
    - "1:A40"
min_workers: 1
max_workers: 10
scale:
  up:
    pressure: 100
    rate: 10s
  down:
    pressure: 10
    rate: 5m
```

Processors can also scale to zero.

```yaml
min_workers: 0
```

Processors can enforce schemas.

```yaml
schema:
  - name: prompt
    type: string
    required: true
```

Send data to a processor stream

```sh
nebu send processor vllm-llama3 --data '{"prompt": "Dlrow Olleh"}'
```

Read data from a processor stream

```text
nebu read processor vllm-llama3 --num 10
```

List all processors

```sh
nebu get processors
```

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
  platform: runpod
  env_vars:
    - key: HELLO
      value: world
  volumes:
    - source: s3://nebulous-rs/test
      dest: /nebu/test
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

### Services [in progress]

Services provide a means to expose containers on a stable IP address.

### Namespaces [in progress]

Namespaces provide a means to segregate groups of resources across clouds.   
   
Resources within a given namespace are network isolated using [Tailnet](https://tailscale.com/kb/1136/tailnet), and can be accessed by simply using thier name as the hostname e.g. `http://foo:8080`.

## Contributing

Please open an issue or submit a PR.

## Inspiration

- [Kubernetes](https://kubernetes.io/)
- [Aurea](https://github.com/aurae-runtime/aurae)
- [RunPod](https://runpod.io/)
- [Prime Intellect](https://primeintellect.com/)
