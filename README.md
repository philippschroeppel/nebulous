# Nebulous

A cross-cloud container orchestrator

Think of it as a kubernetes that can span clouds with a focus on accelerated compute and AI workloads.

## Installation

```sh
curl -fsSL -H "Cache-Control: no-cache" https://storage.googleapis.com/nebulous-rs/releases/install.sh | bash
```

## Usage

Login to the API server
```sh
nebu login
```

Create a container on runpod with 4 A100 GPUs
```sh
nebu create container \
    --name foo \
    --image pytorch/pytorch:latest \
    --cmd "echo hello" \
    --platform runpod \
    --accelerators "4:A100"
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

## Contributing

Please open an issue or submit a PR.