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

#	-e DefaultMapOverride="${DEFAULT_MAP}"
#	-e PartialMapsExpandedOverride="${PARTIAL_MAPS}"
#	-e Layout="${VARIANT}"
pipenv run ./${BuildScript} -c /controller -o ${BUILD_PATH};

OUT_DIR="/mnt/builds/${HASH}"
mkdir -p "${OUT_DIR}"
cp "${BUILD_PATH}/kiibohd.dfu.bin" "${OUT_DIR}"
