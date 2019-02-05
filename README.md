# KiiSrv

Build backend for the configurator

# Setup

[Get Rust](https://rustup.rs/)

`docker-compose build`

**Note:** If you are on arch linux you may need to run the following command first.
`echo N | sudo tee /sys/module/overlay/parameters/metacopy`

A [github access token](https://github.com/settings/tokens) can be stored in the `apikey` file to prevent rate limit exceptions.

# Running

`cargo run`

# Unit Tests

`cargo test`

# Debugging build failues

Getting a shell

`docker-compose run --entrypoint /bin/bash controller-050`

# Upstart

`sudo service kiisrv restart`
`sudo tail -f /var/log/upstart/kiisrv.log`

# Adding build container

- Edit docker-compose.yml
- docker-compose build
- Edit src/main.rs
