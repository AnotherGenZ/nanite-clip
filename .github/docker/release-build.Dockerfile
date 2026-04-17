FROM ubuntu:25.04

ENV DEBIAN_FRONTEND=noninteractive

RUN set -eux; \
    . /etc/os-release; \
    rm -f /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; \
    printf '%s\n' \
        "deb http://archive.ubuntu.com/ubuntu ${VERSION_CODENAME} main universe" \
        "deb http://archive.ubuntu.com/ubuntu ${VERSION_CODENAME}-updates main universe" \
        "deb http://security.ubuntu.com/ubuntu ${VERSION_CODENAME}-security main universe" \
        > /etc/apt/sources.list; \
    apt-get update -o Acquire::Retries=5; \
    apt-get install -y -o Acquire::Retries=5 --no-install-recommends \
        bash \
        build-essential \
        ca-certificates \
        cmake \
        curl \
        extra-cmake-modules \
        git \
        libclang-dev \
        libglib2.0-dev \
        libgtk-3-dev \
        libkf6coreaddons-dev \
        libkf6dbusaddons-dev \
        libkf6globalaccel-dev \
        libpipewire-0.3-dev \
        libxdo-dev \
        ninja-build \
        pkg-config \
        qt6-base-dev; \
    rm -rf /var/lib/apt/lists/*
