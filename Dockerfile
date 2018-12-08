# Dockerfile for KiiConf
# Jacob Alexander 2018

# Ubuntu 18.04 LTS (xenial) base
FROM ubuntu:bionic

LABEL maintainer="haata@kiibohd.com" \
    version="0.1" \
    description="Docker Environment for Kiibohd KiiConf Web Backend"

# Install dependencies
RUN apt-get update && \
    apt-get install -qy locales

RUN apt-get install -qy lighttpd

# Add this git repo
ADD . /KiiConf
WORKDIR /KiiConf

# Prepare lighttpd
# Defaults to test_lighttpd.conf
RUN mkdir -p /var/run/lighttpd && chown www-data:www-data /var/run/lighttpd
RUN touch /var/run/lighttpd.pid && chown www-data:www-data /var/run/lighttpd.pid
ARG lighttpd_conf=lighttpd.conf
ADD ${lighttpd_conf} /etc/lighttpd/lighttpd.conf
EXPOSE 8080 443 1111

# Default command, starting lighttpd
CMD /usr/sbin/lighttpd -D -f /etc/lighttpd/lighttpd.conf

# 4. Run all of KiiConf, using lighttpd inside the docker container
#  docker run kiiconf
# OR (to use localhost instead)
#  docker run -p 127.0.0.1:80:80 kiiconf

# 5. Run all of KiiConf, using lighttpd inside docker container, and detach to the background
#  docker run -d kiiconf
# OR (to use localhost instead)
#  docker run -p 127.0.0.1:80:80 -d kiiconf
