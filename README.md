# KiiSrv

Build backend for the configurator

# Setup

[Get Rust](https://rustup.rs/)

`docker-compose build`

**Note:** If you are on arch linux you may need to run the following command first.

`echo N | sudo tee /sys/module/overlay/parameters/metacopy`

A [github access token](https://github.com/settings/tokens) can be stored in the `apikey` file to prevent rate limit exceptions.

# Running

 - `cargo run`

# Unit Tests

 - `cargo test`

# Debugging build failures

 - Getting a shell:

  `docker-compose run --entrypoint /bin/bash controller-050`

# Upstart

 - `sudo service kiisrv restart`

 - `sudo tail -f /var/log/upstart/kiisrv.log`

# Updating firmware version (controller repo)

## Create a new docker container

 - Edit docker-compose.yml

 - Copy an existing `controler-XXX` section as a template, and use a new name

 - Change `TAG=` to the desired git tag or commit hash.

 - Build the new container. `docker-compose build`

## Update the version dictionary

 - Open src/versions.rs

 - Update `latest => ` (used by the configurator)

 - Update `lts => ` (used by the web configurator)

 - Optional: Add other aliases.
   These may be presented to the configurator as a drop down menu in the future.

## Restart the service

Reference README for instructions. You should see your new container in both
the container list, and the version list.
