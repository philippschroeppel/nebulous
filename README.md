# Nebulous

A cross-cloud container orchestrator

Think of it as a Kubernetes that can span clouds with a focus on accelerated compute and AI workloads. Performant and lightweight via Rust.

## Installation

```sh
curl -fsSL -H "Cache-Control: no-cache" https://storage.googleapis.com/nebulous-rs/releases/install.sh | bash
```

## Usage

Login to an API server
```sh
nebu login
```

Create a container on runpod with 4 A100 GPUs
```yaml
kind: Container
metadata:
  name: pytorch-test
  namespace: nebu-test
image: pytorch/pytorch:latest
command: nvidia-smi
platform: runpod
env_vars:
  - key: HELLO
    value: world
volumes:
  - source: s3://foo/bar
    destination: /quz/baz
    bidirectional: true
    continuous: true
accelerators:
  - "4:A100"
```
```sh
nebu create container -f examples/basic.yaml
```

Create a container on EC2 with 1 L40s GPU
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

Get the IP address of a container
```sh
nebu get containers foo --ip
```

SSH into a container
```sh
nebu ssh foo
```

### Namespaces

Namespaces provide a means to segregate groups of resources across clouds. Resources within a given namespace are network isolated using [Tailnet](https://tailscale.com/kb/1136/tailnet), and can be accessed by simply using thier name as the hostname e.g. `http://foo:8080`.

### Services

Services provide a means to expose containers on a stable IP address.

### Volumes

Volumes provide a means to persist data accross clouds. Nebulous uses [Rclone](https://rclone.org/) to sync data between clouds backed by an object storage provider.

### Organizations

Nebulous is multi-tenant from the ground up. Here is an example of creating a container under the `Agentsea` organization.

```sh
nebu create container \
    --name "Agentsea/foo" \
    --image tensorflow/tensorflow:latest \
    --cmd "echo hello" \
    --platform ec2 \
    --accelerators "1:L40s"
```

### Meters

Nebulous natively supports metered billing through [OpenMeter](https://openmeter.cloud/) using the `meters` field.

```yaml
meters:
  - cost: 0.1
    unit: second
    currency: USD
    metric: runtime 
```

## Contributing

Please open an issue or submit a PR.

## Inspiration

- [Kubernetes](https://kubernetes.io/)
- [Aurea](https://github.com/aurae-runtime/aurae)
- [RunPod](https://runpod.io/)
- [Prime Intellect](https://primeintellect.com/)
