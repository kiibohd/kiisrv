# KiiSrv

Build backend for the configurator

# Setup

[Get Rust](https://rustup.rs/)

`docker-compose build`

**Note:** If you are on arch linux you may need to run the following command first.
`echo N | sudo tee /sys/module/overlay/parameters/metacopy`

# Running

`cargo run`

# Unit Tests

`cargo test`

# Debugging build failues

Getting a shell

`docker-compose run --entrypoint /bin/bash controller-050`
