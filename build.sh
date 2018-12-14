#!/bin/bash
# Rowan Decker 2018
# Arg 1: Build script (keyboard.bash)
# Arg 2: Input dir (kll files)
# Arg 2: Output file
# Env: DefaultMapOverride, PartialMapsExpandedOverride, Layout
#
# Example:
# export DefaultMapOverride="stdFuncMap KType-Standard-0"
# export PartialMapsExpandedOverride="stdFuncMap KType-Standard-0;stdFuncMap KType-Standard-1"
# export Layout="Standard"
# ./build.sh KType.bash my_ktype/kll MyBuild.zip

if [ "$#" -lt 3 ]; then
	echo "Usage: <build script> <input dir> <output file>"
	exit
fi

export PATH="/usr/lib/ccache:$PATH"
export CCACHE_DIR="/mnt/ccache"

# Double check with docker volume mountpoints
CONTROLLER_DIR="/controller"
IN_DIR="${IN_DIR:-/mnt/config}"
OUT_DIR="${OUT_DIR:-/mnt/builds}"

BuildScript="${1}"; shift
KllDir="${IN_DIR}/${1}"; shift
OutFile="${OUT_DIR}/${1}"; shift

echo " @@@@@ DefaultMapOverride=${DefaultMapOverride}"
echo " @@@@@ PartialMapsExpandedOverride=${PartialMapsExpandedOverride}"
echo " @@@@@ Layout=${Layout}"
ls -l "${KllDir}"
echo "------------------------"

BUILD_DIR=$(mktemp -d)
echo " Build Dir: ${BUILD_DIR}"

set -x
mkdir -p "${BUILD_DIR}"

# the kll compiler looks for files in the build dir
mv ${KllDir}/* "${BUILD_DIR}"
rmdir "${KllDir}/${HASH}"

pipenv run ./${BuildScript} -c "${CONTROLLER_DIR}" -o "${BUILD_DIR}" \
 2>&1 | tee "${BUILD_DIR}/build.log"
RETVAL=$?
set +x

if [ $RETVAL -ne 0 ]; then
	OutFile="${OutFile%.*}_error.zip"
fi

echo "------------------------"
echo " @@@@@ Creating build zip ${OutFile}"

cd "${BUILD_DIR}"
mkdir kll
cp *.kll kll/

mkdir log
mv *.log *.h log/

zip -v -r "${OutFile}" *.kll *.dfu.bin *.json kll/ log/
exit $RETVAL
