#!/usr/bin/env bash

set -e



TARGET_OS=$1
shift
if [ -z "$TARGET_OS" ]; then
    echo "Need a target OS"
    exit 1
fi


TARGET_PLATFORM=$1
shift
if [ -z "$TARGET_PLATFORM" ]; then
    echo "Need a TARGET_PLATFORM arg, e.g. x86, or aarch64"
    exit 1
fi

if [[ "$TARGET_OS" =~ macos.* ]]; then
    export TARGET_OS=macos
    export SDKROOT=$(xcrun -sdk macosx --show-sdk-path)
    export MACOSX_DEPLOYMENT_TARGET=$(xcrun -sdk macosx --show-sdk-platform-version)
fi

CROSS_BUILD_TARGET=""
APP_DIR="target/release/"
if [ "$TARGET_PLATFORM" != "x86_64" ];then
    if [ "$TARGET_OS" == "macos" ]; then
        CROSS_BUILD_TARGET="--target=aarch64-apple-darwin"
        APP_DIR="target/aarch64-apple-darwin/release/"
        rustup target add aarch64-apple-darwin
    else
        echo "Don't know how to build $TARGET_PLATFORM on $TARGET_OS"
        exit 1
    fi
fi

if [[ "$TARGET_OS" =~ ubuntu.* ]]; then
    TARGET_OS=linux
    sudo apt install -y musl-tools
    rustup target add x86_64-unknown-linux-musl
    CROSS_BUILD_TARGET="--target=x86_64-unknown-linux-musl"
    APP_DIR="target/x86_64-unknown-linux-musl/release/"
fi


for b in "$@"; do
    set -x
    cargo build $CROSS_BUILD_TARGET --bin $b --release --all-features

    OUTPUT_ASSET_NAME="${b}-$TARGET_OS-${TARGET_PLATFORM}"
    cp $APP_DIR/$b $OUTPUT_ASSET_NAME
    GENERATED_SHA_256=$(shasum -a 256 $OUTPUT_ASSET_NAME | awk '{print $1}')
    echo $GENERATED_SHA_256 > ${OUTPUT_ASSET_NAME}.sha256
    tag_name="${GITHUB_REF##*/}"
    gh release upload "$tag_name" -a $OUTPUT_ASSET_NAME -a ${OUTPUT_ASSET_NAME}.sha256
done

