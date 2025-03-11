#!/bin/bash
set -e

# Configuration
BINARY_NAME="nebulous"
INSTALL_DIR="/usr/local/bin"
BASE_URL="https://storage.googleapis.com/nebulous/releases/latest"

# Strict OS and architecture check
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [ "$OS" != "darwin" ] || [ "$ARCH" != "arm64" ]; then
    echo "Error: This software only supports macOS (darwin) on ARM64 architecture"
    echo "Current system: $OS on $ARCH"
    exit 1
fi

# Construct download URL
PACKAGE_NAME="${BINARY_NAME}-latest-${OS}-${ARCH}.tar.gz"
DOWNLOAD_URL="${BASE_URL}/${PACKAGE_NAME}"

echo "Downloading ${BINARY_NAME} for ${OS}-${ARCH}..."

# Create temporary directory
TMP_DIR=$(mktemp -d)
trap 'rm -rf -- "$TMP_DIR"' EXIT

# Download and verify the binary
echo "Downloading from: $DOWNLOAD_URL"
curl -fsSL "${BASE_URL}/checksums.txt" > "$TMP_DIR/checksums.txt"
curl -fsSL "$DOWNLOAD_URL" > "$TMP_DIR/$PACKAGE_NAME"

# Extract and install the binary
echo "Extracting archive..."
tar -xzvf "$TMP_DIR/$PACKAGE_NAME" -C "$TMP_DIR" || {
    echo "Failed to extract archive"
    exit 1
}

# Ensure install directory exists and is writable
if [ ! -d "$INSTALL_DIR" ]; then
    echo "Creating install directory: $INSTALL_DIR"
    sudo mkdir -p "$INSTALL_DIR" || {
        echo "Failed to create install directory"
        exit 1
    }
fi

# Install the binary
echo "Installing binary to $INSTALL_DIR..."
sudo mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/" || {
    echo "Failed to move binary to install directory"
    exit 1
}

echo
echo "${BINARY_NAME} has been installed to $INSTALL_DIR"
echo "Run '${BINARY_NAME} --help' to get started"