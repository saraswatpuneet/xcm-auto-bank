#!/bin/bash

set -e
command=${1:-run_full}

DIR=$(dirname $BASH_SOURCE)
ROOT=$(realpath $DIR/..)

# Amount of validators in relay chain
VALIDATORS=4
# this port will be used as start port to configure validators:
# If there are 4 validators, they will have "--ws-port=": 9950,9951,9952,9953
WSPORT=9950

# Read configuration: commit hash of needed revision, initial node keys, chaindata persistance parameter, etc...
. ${DIR}/config.sh

# print executable version without platform part
function get_ver() {
  $1 -V | sed -e 's/^\(.*[[:space:]]\)\([0-9.]*\)\(-[[:xdigit:]]\{9\}\)\?\b.*/\2\3/'
}

# This function installs apt packets, Python interface to Substrate, setup Rust and WASM target for it
function install_tools() {
  local fname="${ROOT}/bin/.modified"

  if [ -f $fname ] && [ "${RUST_TOOLCHAIN} ${POLKADOT_COMMIT}" == "$(<$fname)" ]; then
    return 0
  fi

  apt update
  apt install -y --no-upgrade git curl python3 python3-pip cmake pkg-config libssl-dev git build-essential clang libclang-dev libz-dev

  python3 -c 'import substrateinterface;' || {
    echo "[SETUP] Installing Python substrate-interface module"
    python3 -m pip install -r ${DIR}/requirements.txt
  }

  if [ ! -f $HOME/.cargo/evn ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
      sh -s -- -y --profile default -t wasm32-unknown-unknown --default-toolchain ${RUST_TOOLCHAIN}

    source $HOME/.cargo/env
    # uncomment next line to set installed toolchain as a default
    # rustup default ${RUST_TOOLCHAIN}
  else
    source $HOME/.cargo/env
    rustup toolchain install ${RUST_TOOLCHAIN} --allow-downgrade --profile default --component clippy
    rustup target add wasm32-unknown-unknown --toolchain ${RUST_TOOLCHAIN}
  fi
  echo "${RUST_TOOLCHAIN} ${POLKADOT_COMMIT}" > $fname
}

# Build:
# - binary for relay chain (for fixed Polkadot revision)
# - binary for parachain collators

if [ "$command" != "help" ]; then
  #get last modified source file
  MODIFIED=$(find ${ROOT}/ -type f -name '*.rs' -printf '%T@ %p\n' | sort -nr | head -1 | cut -d' ' -f2-)

  mkdir -p ${ROOT}/var
  mkdir -p ${ROOT}/bin

  install_tools

  # Build POLKADOT_COMMIT revision of "https://github.com/paritytech/polkadot"
  if [[ ! -f "${ROOT}/bin/${RELAY_NODE}" ]]; then
    echo "[SETUP] Build and install polkadot ${green}relay chain node${reset}"
    cargo +${RUST_TOOLCHAIN} install --locked --root ${ROOT} --rev ${POLKADOT_COMMIT} --git https://github.com/paritytech/polkadot polkadot
  fi

  # Build collators' binary
  if [ ! -x "${ROOT}/bin/${CLIENT_COLLATOR}" ] || [ "${ROOT}/bin/${CLIENT_COLLATOR}" -ot "${MODIFIED}" ]; then
    echo "[SETUP] Build and install ${green}client collator node${reset}"

    if [ ${BACKUP} -eq 1 ] && [ -x "${ROOT}/bin/${CLIENT_COLLATOR}" ]; then
      mv "${ROOT}/bin/${CLIENT_COLLATOR}" "${ROOT}/bin/${CLIENT_COLLATOR}.bak"
    fi

    cargo +${RUST_TOOLCHAIN} install --locked --root ${ROOT} --path ${ROOT}/node --features client --bin ${CLIENT_COLLATOR}
  fi

  echo  "[SETUP] ${green}${CLIENT_COLLATOR}${reset}"
  get_ver "${ROOT}/bin/${CLIENT_COLLATOR}"
  #echo "[SETUP] ${green}${ver}${reset}"

  if [ ! -x "${ROOT}/bin/${SERVICE_COLLATOR}" ] || [ "${ROOT}/bin/${SERVICE_COLLATOR}" -ot "${MODIFIED}" ]; then
    echo "[SETUP] Build and install ${green}client collator node${reset}"

    if [ ${BACKUP} -eq 1 ] && [ -x "${ROOT}/bin/${SERVICE_COLLATOR}" ]; then
      mv "${ROOT}/bin/${SERVICE_COLLATOR}" "${ROOT}/bin/${SERVICE_COLLATOR}.bak"
    fi

    cargo +${RUST_TOOLCHAIN} install --locked --root ${ROOT} --path ${ROOT}/node --features service --bin ${SERVICE_COLLATOR}
  fi

  echo  "[SETUP] ${green}${SERVICE_COLLATOR}${reset}"
  get_ver "${ROOT}/bin/${SERVICE_COLLATOR}"
  #echo "[SETUP] ${green}${ver}${reset}"

  #configure relay config
  if [ ! -f "${ROOT}/config/spec.json" ]; then
    echo "[SETUP] create link to relay spec './config/spec.json'"
    ln -s "${ROOT}/config/rococo.beefy.json" "${ROOT}/config/spec.json"
  fi

fi

# Setup multiprocess environment

# Variables, storing launched processes pids, I/O pipes and node counter "node_index"
# Each new launched node will increase "node_index"
node_index=0
declare -a node_pids
declare -a node_pipes

function make_sed_expr() {
  name="$1"
  type="$2"
  printf "s/^/%8s %s: /" "$name" "$type"
}

function cleanup() {
  pkill -TERM -g $$
  tput sgr0
  _no_more_locking
}

function run_chain() {
  local ver=$(get_ver "${ROOT}/bin/${RELAY_NODE}")

  if [ "$ver" != "${RELAY_VER}" ]; then
    echo "${red}WARNING${reset}relay node version '${ver}' doesn't match ${red}'${RELAY_VER}'${reset}"
  fi

  echo "[SETUP] It will start ${VALIDATORS} validator nodes and 2 parachain nodes (paraid 100 and 200)"
  echo "[SETUP] use next parachain addresses to access accounts in parachains and relay chains"
  echo "[SETUP] in relay chain  parachain 100 address ${blue}5Ec4AhP7HwJNrY2CxEcFSy1BuqAY3qxvCQCfoois983TTxDA${reset}"
  echo "[SETUP] in relay chain  parachain 200 address ${blue}5Ec4AhPTL6nWnUnw58QzjJvFd3QATwHA3UJnvSD4GVSQ7Gop${reset}"
  echo "[SETUP] in parachain 100  parachain 200 address ${blue}5Eg2fntGQpyQv5X5d8N5qxG4sX5UBMLG77xEBPjZ9DTxxtt7${reset}"
  echo "[SETUP] in parachain 200  parachain 100 address ${blue}5Eg2fnsvNfVGz8kMWEZLZcM1AJqqmG22G3r74mFN1r52Ka7S${reset}"
  echo ""

  echo "[SETUP] Print validators websocket RPC endpoints:"
  for i in $(seq 1 $VALIDATORS); do
    echo "[SETUP] validator ${i}: ${blue}ws://localhost:$((WSPORT + i - 1))/${reset}"
  done
  echo "[SETUP] Parachain(100) endpoint: ${blue}ws://localhost:$((WSPORT + VALIDATORS + 100))/${reset}"
  echo "[SETUP] Parachain(200) endpoint: ${blue}ws://localhost:$((WSPORT + VALIDATORS + 101))/${reset}"

  echo "${green}https://polkadot.js.org/apps/?rpc=ws://localhost:${WSPORT}/${reset}"

  read -n 1 -s -r -p "Press any key to continue. To stop Ctrl+C"
  
  # generate parachain wasm(runtime code) and genesis states for parachains 100 and 200
  echo "[SETUP] Generating WASM runtime for parachains 100 to ${ROOT}/var/client.wasm"
  ${ROOT}/bin/${CLIENT_COLLATOR} export-genesis-wasm  --decompress >${ROOT}/var/client.wasm
  echo "[SETUP] Exporting genesis state for parachain 100 to ${ROOT}/var/100.gen"
  ${ROOT}/bin/${CLIENT_COLLATOR} export-genesis-state --parachain-id 100 ${ROOT}/var/100.gen

  echo "[SETUP] Generating WASM runtime for parachains 200 to ${ROOT}/var/service.wasm"
  ${ROOT}/bin/${SERVICE_COLLATOR} export-genesis-wasm --decompress >${ROOT}/var/service.wasm
  echo "[SETUP] Exporting genesis state for parachain 200 to ${ROOT}/var/200.gen"
  ${ROOT}/bin/${SERVICE_COLLATOR} export-genesis-state --parachain-id 200 ${ROOT}/var/200.gen

  # on exit kill child process and remove temporary files
  trap cleanup EXIT

  # Start $VALIDATORS relay nodes(relay chain has 4 validators.)
  local i=0
  for name in 'alice' 'bob' 'dave' 'charlie'; do
    ((i+1))
    run_node 'relay' $name  $i

    if [[ $i -eq ${VALIDATORS} ]]; then
        break
    fi
  done

  #run_node 'relay' 'alice'   1
  #run_node 'relay' 'bob'     2
  #run_node 'relay' 'dave'    3
  #run_node 'relay' 'charlie' 4

  # Start 2 parachain nodes(each parachain has 1 collator)
  run_node 'client' 'alice'  100
  run_node 'service' 'bob'   200

  # store child process pid in .pids file
  rm -rf ${ROOT}/var/.pids

  for node_pid in "${node_pids[@]}"; do
    echo ${node_pid} >>  ${ROOT}/var/.pids
  done
}

# Run relay chain validator, or parachain collator.
# Uses global "node_index" as counter to configure non-conflicting endpoints
# Each launched node STDOUT/STDERR are redirected to separate pipes
# Each launched node uses ${BOOT_NODE} as boot node
function run_node() {
  relay_or_chain=$1
  local name=$2
  local paraid=${3:-1}

  local stdout
  local stderr
  #stdout=$(mktemp --dry-run --tmpdir)
  #stderr=$(mktemp --dry-run --tmpdir)
  #mkfifo "$stdout"
  #mkfifo "$stderr"

  #node_pipes+=("$stdout")
  #node_pipes+=("$stderr")

  local port=$((30400 + node_index))
  local ws_port=$((WSPORT + node_index))

  case $relay_or_chain in
  relay)
    run_relay_node $node_index $name $port $ws_port $paraid $stdout $stderr
    ;;
	
  client)
    run_para_node $node_index $name $port $ws_port $paraid ${CLIENT_COLLATOR} $stdout $stderr
    ;;

  service)
    run_para_node $node_index $name $port $ws_port $paraid ${SERVICE_COLLATOR} $stdout $stderr
    ;;

  *)
    echo "unknown node type '$relay_or_chain'"
    exit 1
    ;;
  esac

  node_pids+=("$!")

  echo "run ${blue} ${relay_or_chain} ${name}${reset} on ${blue}localhost:${ws_port}${reset}"
  #echo " log $stdout"
  #echo " err $stderr"

  node_index=$((node_index + 1))
  # send output from the stdout pipe to stdout, prepending the node name
  # sed -e "$(make_sed_expr "$name" "OUT")" "$stdout" >&1 &
  # send output from the stderr pipe to stderr, prepending the node name
  # sed -e "$(make_sed_expr "$name" "ERR")" "$stderr" >&2 &
}

function run_relay_node() {
  local node_id=$1
  local node_name=$2
  local port=$3
  local ws_port=$4
  local paraid=$5
  local stdout=$6
  local stderr=$7

  local title="${green}[relay  $paraid]${reset} "
  local boot="--bootnodes /ip4/127.0.0.1/tcp/30400/p2p/${BOOT_NODE}"

  if [ $node_id -eq 0 ]; then
    boot="--node-key ${NODE_KEY}"
  fi

  if [ ${PERSISTENT} -eq 1 ]; then
    mkdir -p ${ROOT}/var/${node_id}
    base="base-path ${ROOT}/var/${node_id}"
  else
    base="tmp"
  fi

  "${ROOT}/bin/${RELAY_NODE}" \
    --chain "${ROOT}/config/spec.json" \
    -l"$RELAY_LOGCFG" \
    --$name \
    --$base \
    --port $port \
    --ws-port $ws_port \
    --rpc-cors all \
    $boot \
	2>&1 | sed "s/^/$title/" &
  #1>"$stdout" 2>"$stderr" &

}

function run_para_node() {
  local node_id=$1
  local node_name=$2
  local port=$3
  local ws_port=$4
  local paraid=$5
  local para_type=$6
  local stdout=$7
  local stderr=$8

  local title=''
  local logcfg='info'
  if [ "${para_type}" == "${CLIENT_COLLATOR}" ]; then
    title="${blue}[para $paraid]${reset} "
    logcfg="${CLIENT_LOGCFG}"
  else
    title="${blue2}[para $paraid]${reset} "
    logcfg="${SERVICE_LOGCFG}"
  fi

  local boot="--bootnodes /ip4/127.0.0.1/tcp/30400/p2p/${BOOT_NODE}"

  if [ ${PERSISTENT} -eq 1 ]; then
    mkdir -p ${ROOT}/var/${node_id}
    base="base-path ${ROOT}/var/${node_id}"
  else
    base="tmp"
  fi

  "${ROOT}/bin/${para_type}" \
    --$name \
    --collator \
    -l"$logcfg" \
    --parachain-id $paraid \
    --no-telemetry \
    --$base \
    --port $((100 + port)) \
    --ws-port $((100 + ws_port)) \
    --rpc-cors all \
    -- \
    --execution Wasm \
    $boot \
    --port "$port" \
    --ws-port "$ws_port" \
    --chain "${ROOT}/config/spec.json" \
	2>&1 | sed "s/^/$title/"  &
  #1>"$stdout" 2>"$stderr" &
}

function register_parachain() {
  python3 ${DIR}/register.py 'endow' \
      --ws_url "ws://localhost:${WSPORT}/"

  echo "[SETUP] Registering 100 parachain"
  python3 ${DIR}/register.py 'register' \
      --ws_url "ws://localhost:${WSPORT}/" \
      --paraid 100 \
      --wasm ${ROOT}/var/client.wasm \
      --genesis ${ROOT}/var/100.gen

  echo "[SETUP] Waiting for 20 sec"
  sleep 20
  echo "[SETUP] registering 200 parachain"
  python3 ${DIR}/register.py 'register' \
       --ws_url "ws://localhost:${WSPORT}/" \
       --paraid 200 \
       --wasm ${ROOT}/var/service.wasm \
       --genesis ${ROOT}/var/200.gen
}

case $command in
run)
  _prepare_locking
  _lock xn || exit 1
  echo "[RUN] chain"
  run_chain
  wait
  ;;

run_full)
  _prepare_locking
  _lock xn || exit 1

  run_chain
  sleep 10
  register_parachain
  sleep 60
  echo "[SETUP] Open HRMP channel"
  python3 ${DIR}/register.py 'hrmp_open' \
    --ws_url "ws://localhost:${WSPORT}/" --paraid 100 200

  python3 ${DIR}/register.py 'hrmp_open' \
    --ws_url "ws://localhost:${WSPORT}/" --paraid 200 100

  wait
  ;;

clean)
  _prepare_locking
  _lock xn || exit 1
  for i in $(seq 1 $VALIDATORS); do rm -rf "${ROOT}/var/${i}"; done
  
  rm "${ROOT}/bin/.modified"
  ;;

register)
  register_parachain
  ;;

hrmp)
  echo "endow parachain accounts"
  python3 ${DIR}/register.py 'endow_para' \
    --ws_url "ws://localhost:$((WSPORT + VALIDATORS + 100 + 1))/" \
    --paraid 100
  echo "open hrmp chain"
  python3 ${DIR}/register.py 'hrmp_open' \
    --ws_url "ws://localhost:${WSPORT}/"

  ;;

build)
  echo "parachain's built and ready to run"
  ;;

test)
  cargo test
  ;;
*)
  echo "${green}build${reset}     - install toolchain and build projects"
  echo "${green}run${reset}       - run relay chain and parachain nodes "
  echo "${green}run_full${reset}  - run relay chain and parachain nodes and register two parachains "
  echo "${green}register${reset}  - register two parachains with id 100 and 200"
  echo "${green}ump${reset}       - send Balance transfer UMP message from 100 parachain"
  echo "${green}hrmp${reset}      - register channel between 100 and 200 parachains"
  echo "${green}clean${reset}     - delete nodes databases (for persistent mode only)"
  echo "            and pass Balance transfer HRMP message from 100 to 200 parachain "
  echo "${green}hrmpm${reset}     - pass  Balance transfer HRMP message,"

  ;;
esac