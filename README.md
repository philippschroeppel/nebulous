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
name: pytorch-test
image: "pytorch/pytorch:latest"
command: "nvidia-smi"
platform: "runpod"
namespace: "foo"
labels:
  this: that
env_vars:
  TEST: "hello"
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

Nebulous natively supports metered billing through [OpenMeter](https://openmeter.cloud/) using the `cost` field.

```sh
nebu create container \
    --name "Agentsea/foo" \
    --image tensorflow/tensorflow:latest \
    --cmd "echo hello" \
    --platform ec2 \
    --accelerators "1:L40s"
    --cost "0.1/s"
```
_Cost is in USD_

## Contributing

Please open an issue or submit a PR.

## Inspiration

- [Kubernetes](https://kubernetes.io/)
- [Aurea](https://github.com/aurae-runtime/aurae)
- [RunPod](https://runpod.io/)