# Nebulous

A cross-cloud container orchestrator

Think of it as a kubernetes that can span clouds with a focus on accelerated compute and AI workloads.

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
  - source: s3://nebulous-rs/test
    destination: /nebu/test
    bidirectional: true
    continuous: true
    resync: false
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

List one container
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

Nebulous is multi-tenant from the ground up. Here is an example of creating a container under the `Agentsea` organization.
```sh
nebu create container \
    --name "Agentsea/foo" \
    --image tensorflow/tensorflow:latest \
    --cmd "echo hello" \
    --platform ec2 \
    --accelerators "1:L40s"
```

## Contributing

Please open an issue or submit a PR.