#!/bin/bash
# Jacob Alexander 2018
# Arg 1: Build Directory
# Arg 2: Scan Module
# Arg 3: DefaultMap
# Arg 4: Layer 1
# Arg 5: Layer 2
# etc.
# Example: ./build_layout.bash <hash> <scan module> <variant> <default map> <layer1> <layer2>
#          ./build_layout.bash c3184563548ed992bfd3574a238d3289 MD1 "" MD1-Hacker-0.kll MD1-Hacker-1.kll
#          ./build_layout.bash c3184563548ed992bfd3574a238d3289 MD1 "" "" MD1-Hacker-1.kll
# NOTE: If a layer is blank, set it as ""

if [ "$#" -lt 4 ]; then
	echo "Usage: <hash> <scan module> <variant> <default map> <layers...>"
	exit
fi

export PATH="/usr/lib/ccache:$PATH"
export CCACHE_DIR="/mnt/ccache"

# Double check with docker volume mountpoints
IN_DIR="/mnt/kll"
OUT_DIR="/mnt/builds"

# Takes a layer path and prints the name(s) in cmake format
# "layer1 layer1a"
# Arg 1: List of file paths
layer() {
	output=""
	for file in $@; do
		filename=$(basename "${file}")
		extension="${filename##*.}"
		filename_base="${filename%.*}"
		output="${output}${filename_base} "
	done

	# Output everything except the last character unless there was nothing in this layer
	if [ "${output}" == "" ]; then
		echo ""
	else
		echo "${output::${#output}-1}"
	fi
}

HASH="${1}"; shift
BUILD_PATH="/tmp/${HASH}"
mkdir -p "${BUILD_PATH}"

SCAN_MODULE="${1}"; shift
VARIANT="${1}"; shift

ExtraMap="stdFuncMap"
DEFAULT_MAP="${ExtraMap} $(layer ${1})"

PARTIAL_MAPS="${ExtraMap} $(layer ${1})"
shift
while (( "$#" >= "1" )); do
	PARTIAL_MAPS="${PARTIAL_MAPS};${ExtraMap} $(layer ${1})"
	shift
done

case "$SCAN_MODULE" in
"MD1")      # Infinity
	BuildScript="infinity.bash"
	;;
"MD1.1")    # Infinity LED
	BuildScript="infinity_led.bash"
	;;
"MDErgo1")  # Ergodox
	BuildScript="ergodox.bash"
	ExtraMap="infinity_ergodox/lcdFuncMap"
	;;
"WhiteFox") # WhiteFox
	BuildScript="whitefox.bash"
	;;
"KType")    # K-Type
	BuildScript="k-type.bash"
	;;
"Kira")     # Kira
	BuildScript="kira.bash"
	;;
*)
	echo "ERROR: Unknown keyboard type"
	exit 1
	;;
esac

export DefaultMapOverride="${DEFAULT_MAP}"
export PartialMapsExpandedOverride="${PARTIAL_MAPS}"
export Layout="${VARIANT}"

echo " @@@@@ DefaultMapOverride=${DefaultMapOverride}"
echo " @@@@@ PartialMapsExpandedOverride=${PartialMapsExpandedOverride}"
echo " @@@@@ Layout=${Layout}"
ls -l "${IN_DIR}/${HASH}"
echo "------------------------"

# the kll compiler looks for files in the build dir
mv ${IN_DIR}/${HASH}/* "${BUILD_PATH}"
rmdir "${IN_DIR}/${HASH}"

pipenv run ./${BuildScript} -c /controller -o "${BUILD_PATH}" \
 2>&1 | tee "${BUILD_PATH}/build.log"
RETVAL=$?

if [ $RETVAL -eq 0 ]; then
	ZIP_NAME="${HASH}.zip"
else
	ZIP_NAME="${HASH}_error.zip"
fi

echo "------------------------"
OUT_FILE="${OUT_DIR}/${ZIP_NAME}"
echo " @@@@@ Creating build zip ${OUT_FILE}"

cd "${BUILD_PATH}"
zip -v "${OUT_FILE}" *.kll *.dfu.bin *.log *.h *.json
