#!/bin/bash
CACHE_PATH="$(python3 -m kll --layout-cache-refresh)"
CACHE_NAME="$(basename ${CACHE_PATH})"

# Move cache dir into a shared location
CACHE_MOUNT="/kll_cache"
mkdir -p "${CACHE_MOUNT}"
rm -rf "${CACHE_MOUNT}/${CACHE_NAME}"
mv "${CACHE_PATH}" "${CACHE_MOUNT}"

echo "max_size = 5.0G" > /mnt/ccache/ccache.conf
