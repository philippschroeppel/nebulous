#!/bin/bash

set -e

# Configuration
PROJECT_NAME="nebulous"
GCS_BUCKET="nebulous"
GCS_PATH="releases"

# Detect version from Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | cut -d '"' -f 2)
if [ -z "$VERSION" ]; then
    echo "No version found in Cargo.toml"
    exit 1
fi

echo "Building version: $VERSION"

# Install cross-compilation tools if needed
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
rustup target add x86_64-pc-windows-gnu

# Build function
build_and_upload() {
    local ARCH=$1
    local OS=$2
    local TARGET=$3
    local EXT=$4

    echo "Building for $ARCH-$OS..."
    
    # Build the binary
    cross build --release --target $TARGET
    
    # Create archive name
    local ARCHIVE_NAME="${PROJECT_NAME}-${VERSION}-${OS}-${ARCH}${EXT}"
    local LATEST_ARCHIVE_NAME="${PROJECT_NAME}-latest-${OS}-${ARCH}${EXT}"
    local BINARY_PATH="target/${TARGET}/release/${PROJECT_NAME}"
    
    # Add extension for Windows
    if [ "$OS" = "windows" ]; then
        BINARY_PATH="${BINARY_PATH}.exe"
    fi
    
    # Create archive
    if [ "$OS" = "windows" ]; then
        zip -j "${ARCHIVE_NAME}" "${BINARY_PATH}"
    else
        tar -czf "${ARCHIVE_NAME}" -C "target/${TARGET}/release/" "${PROJECT_NAME}"
    fi
    
    # Upload to GCS versioned path
    echo "Uploading ${ARCHIVE_NAME} to GCS..."
    gsutil cp "${ARCHIVE_NAME}" "gs://${GCS_BUCKET}/${GCS_PATH}/${VERSION}/${ARCHIVE_NAME}"
    gsutil acl ch -u AllUsers:R "gs://${GCS_BUCKET}/${GCS_PATH}/${VERSION}/${ARCHIVE_NAME}"
    
    # Upload to GCS latest path
    echo "Uploading to latest path..."
    gsutil cp "${ARCHIVE_NAME}" "gs://${GCS_BUCKET}/${GCS_PATH}/latest/${LATEST_ARCHIVE_NAME}"
    gsutil acl ch -u AllUsers:R "gs://${GCS_BUCKET}/${GCS_PATH}/latest/${LATEST_ARCHIVE_NAME}"
    
    # Cleanup
    rm "${ARCHIVE_NAME}"
}


# Build matrix
echo "Starting builds..."

# Linux builds
build_and_upload "x86_64" "linux" "x86_64-unknown-linux-musl" ".tar.gz"
build_and_upload "aarch64" "linux" "aarch64-unknown-linux-musl" ".tar.gz"

# macOS builds
build_and_upload "x86_64" "darwin" "x86_64-apple-darwin" ".tar.gz"
build_and_upload "aarch64" "darwin" "aarch64-apple-darwin" ".tar.gz"

# Windows build
build_and_upload "x86_64" "windows" "x86_64-pc-windows-gnu" ".zip"

# Create and upload checksums
echo "Generating checksums for version path..."
cd "gs://${GCS_BUCKET}/${GCS_PATH}/${VERSION}/"
sha256sum * > checksums.txt
gsutil cp checksums.txt "gs://${GCS_BUCKET}/${GCS_PATH}/${VERSION}/checksums.txt"
gsutil acl ch -u AllUsers:R "gs://${GCS_BUCKET}/${GCS_PATH}/${VERSION}/checksums.txt"

# Create and upload checksums for latest path
echo "Generating checksums for latest path..."
cd "gs://${GCS_BUCKET}/${GCS_PATH}/latest/"
sha256sum * > checksums.txt
gsutil cp checksums.txt "gs://${GCS_BUCKET}/${GCS_PATH}/latest/checksums.txt"
gsutil acl ch -u AllUsers:R "gs://${GCS_BUCKET}/${GCS_PATH}/latest/checksums.txt"

echo "Build and upload complete for version ${VERSION}"