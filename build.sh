#!/bin/bash
# Rowan Decker 2018
# Arg 1: Build script (keyboard.bash)
# Arg 2: Input dir (kll files)
# Arg 2: Output file
# Env: DefaultMapOverride, PartialMapsExpandedOverride, Layout
#      SPLIT_KEYBOARD
#
# Example:
# export DefaultMapOverride="stdFuncMap KType-Standard-0"
# export PartialMapsExpandedOverride="stdFuncMap KType-Standard-0;stdFuncMap KType-Standard-1"
# export Layout="Standard"
# ./build.sh KType.bash my_ktype/kll MyBuild.zip

build() {
	BuildScript="$1"
	BUILD_DIR="$2"
	(echo " Build Dir: ${BUILD_DIR}"

	set -x
	mkdir -p "${BUILD_DIR}"

	pipenv run ./${BuildScript} -c "${CONTROLLER_DIR}" -o "${BUILD_DIR}" \
	 2>&1 | tee "${BUILD_DIR}/build.log"

	set +x
	)
}

if [ "$#" -lt 3 ]; then
	echo "Usage: <build script> <input dir> <output file>"
	exit
fi

# Double check with docker volume mountpoints
CONTROLLER_DIR="/controller"
BUILD_DIR="$(mktemp -d)"
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

# try to use ccache
export PATH="/usr/lib/ccache:$PATH"

# try to use a cached kll layouts dir
for f in /kll_cache/*; do ln -s "$f" /tmp/; done

# try to use a github apikey secret
[ -z "$GITHUB_APIKEY" ] && export GITHUB_APIKEY="$(cat /run/secrets/github_apikey)"

set -x
# the kll compiler looks for files in the build dir
mv ${KllDir}/* "${BUILD_DIR}"
rmdir "${KllDir}/${HASH}"

if [ "${SPLIT_KEYBOARD}" == "1" ]; then
	LBuildPath="${BUILD_DIR}/left"
	build "${BuildScript%.*}-l.bash" "${LBuildPath}" &
	PID_LEFT=$!

	RBuildPath="${BUILD_DIR}/right"
	build "${BuildScript%.*}-r.bash" "${RBuildPath}" &
	PID_RIGHT=$!

	wait $PID_LEFT $PID_RIGHT
	RETVAL=$?

	ln -s ${LBuildPath}/build.log ${BUILD_DIR}/build.log
	ln -s ${LBuildPath}/kiibohd.dfu.bin ${BUILD_DIR}/left_kiibohd.dfu.bin
	ln -s ${LBuildPath}/kiibohd.secure.dfu.bin ${BUILD_DIR}/left_kiibohd.secure.dfu.bin
	ln -s ${LBuildPath}/kll.json ${BUILD_DIR}/left_kll.json
	ln -s ${LBuildPath}/generatedKeymap.h ${BUILD_DIR}/left_generatedKeymap.h
	ln -s ${LBuildPath}/kll_defs.h ${BUILD_DIR}/left_kll_defs.h
	ln -s ${RBuildPath}/build.log ${BUILD_DIR}/build.log
	ln -s ${RBuildPath}/kiibohd.dfu.bin ${BUILD_DIR}/right_kiibohd.dfu.bin
	ln -s ${RBuildPath}/kiibohd.secure.dfu.bin ${BUILD_DIR}/right_kiibohd.secure.dfu.bin
	ln -s ${RBuildPath}/kll.json ${BUILD_DIR}/right_kll.json
	ln -s ${RBuildPath}/generatedKeymap.h ${BUILD_DIR}/right_generatedKeymap.h
	ln -s ${RBuildPath}/kll_defs.h ${BUILD_DIR}/right_kll_defs.h

else
	build "${BuildScript}" "${BUILD_DIR}"
	RETVAL=$?
fi

if [ "$RETVAL" -ne 0 ]; then
	OutFile="${OutFile%.*}_error.zip"
fi

ls -la "${BUILD_DIR}"

echo "------------------------"
echo " @@@@@ Creating build zip ${OutFile}"

cd "${BUILD_DIR}"
mkdir kll
cp *.kll kll/

mkdir log
mv *.log *.h log/

zip -v "${OutFile}" *.kll *.dfu.bin *.json kll/* log/*
exit $RETVAL
